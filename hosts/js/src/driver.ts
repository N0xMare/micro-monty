/**
 * The async loop a code-mode host runs on a {@link Session}: tools are async
 * JavaScript functions, the cell calls them as awaitables (`await tool(...)`,
 * `asyncio.gather(...)`), and CPU is bounded by fuel slices.
 *
 * - A host call to an async tool is answered `{ kind: "future" }` and the tool
 *   starts at once, so a fan-out runs its tools concurrently. When the cell
 *   waits, every settled answer among the calls it waits on goes back in one
 *   batch; if none has settled, the driver awaits the first.
 * - A tool listed in `sync` is answered with its value directly (`tool(...)`
 *   without `await` in the cell); it must not return a promise.
 * - With `fuelSlice`, the cell runs in slices of that much fuel and the driver
 *   yields to the event loop between slices (`yieldEvery`), so one busy cell does
 *   not starve other requests of the isolate. Before each new slice it reads
 *   `fuelWanted()`: a native operation that needs more than the budget left stops
 *   the cell before it starts (the reference host's advice). The budget counts what
 *   the engine charged, overdrafts included: a native that charges after it runs
 *   (`str.replace`, `json.loads`, ...) can take more than the slice in one step, and
 *   the next slice is then refused. One such step is the most a cell runs past its
 *   budget. While it slices, it turns on `setGcYield` (when the guest has it), so a
 *   garbage collection step ends a slice too, without fuel charged for it.
 */

import { type HostError, type HostValue, MmError, type Outcome, type Progress, type Session } from "./host.ts";

export interface ToolCall {
	call: number;
	name: string;
	args: HostValue[];
	kwargs: Map<string, HostValue>;
}

export type Tool = (call: ToolCall) => unknown;

export interface RunOptions {
	/** Tools by the host-call name the guest reports (`module.function`, or `function` for builtins). */
	tools: Record<string, Tool>;
	/** Tools answered synchronously. */
	sync?: ReadonlySet<string>;
	filename?: string;
	/** Fuel per slice; `undefined` runs without slices (unlimited unless the session has fuel). */
	fuelSlice?: number;
	/** Fuel the whole cell may use; past it the cell is cancelled and `run` returns `budget`. */
	fuelBudget?: number;
	/** Yields between slices; default a macrotask (`setTimeout(0)`), which lets other requests run. */
	yieldEvery?: () => Promise<void>;
	/**
	 * Called when the cell parks for a request or a deadline, or waits on calls the
	 * driver did not start (a restored session). Return the resume, or `undefined` to
	 * end the run there and leave the session stopped (`stopped`).
	 */
	onStop?: (p: Progress) => import("./abi.ts").Resume | undefined;
}

/** How a run ended; with `fuelSlice`, `fuel` is the fuel the cell used (else 0). */
export type RunResult = { slices: number; fuel: number } & (
	| { kind: "complete"; progress: Extract<Progress, { kind: "complete" }> }
	| { kind: "error"; progress: Extract<Progress, { kind: "error" }> }
	| { kind: "budget" }
	| { kind: "stopped"; progress: Progress }
);

/** Turns a thrown tool error into the host error the cell sees (`e.kind` names the Python class). */
export function hostError(e: unknown): HostError {
	if (e && typeof e === "object") {
		const o = e as { kind?: unknown; message?: unknown; data?: unknown; cls?: unknown };
		const err: HostError = {
			kind: typeof o.kind === "string" ? o.kind : "RuntimeError",
			message: typeof o.message === "string" ? o.message : String(e),
		};
		if (o.data !== undefined) err.data = o.data as HostValue;
		if (typeof o.cls === "string") err.cls = o.cls;
		return err;
	}
	return { kind: "RuntimeError", message: String(e) };
}

const macrotask = () => new Promise<void>((r) => setTimeout(r, 0));

interface Open {
	done: Outcome | undefined;
	promise: Promise<void>;
}

/** The fuel total the driver sets for a run when the session has none (2^52). */
const UNCAPPED = 2 ** 52;

/** Runs one cell to its end (or a stop `onStop` declines) and returns how it ended. */
export async function run(session: Session, source: string, o: RunOptions): Promise<RunResult> {
	const open = new Map<number, Open>();
	const slice = o.fuelSlice;
	let slices = 1;
	// Fuel is counted as the engine charged it, from the session's fuel total: a native
	// that charges after it runs can overdraw a slice, and `fuel()` stops at 0, so what
	// was granted undercounts it. Without a total of the host's own, the run sets one
	// and clears it again when it returns.
	const ownTotal = slice !== undefined && session.fuelTotal() === null;
	if (ownTotal) session.setFuelTotal(UNCAPPED);
	const gcYield = slice !== undefined && session.setGcYield(true);
	try {
		const total0 = session.fuelTotal() ?? 0;
		const fuel = () => (slice === undefined ? 0 : total0 - (session.fuelTotal() ?? total0));
		if (slice !== undefined) session.setFuel(Math.min(slice, o.fuelBudget ?? slice));
		let p = session.start(source, o.filename ?? "cell.py");
		for (;;) {
			switch (p.kind) {
				case "complete":
					return { kind: "complete", progress: p, slices, fuel: fuel() };
				case "error":
					return { kind: "error", progress: p, slices, fuel: fuel() };
				case "host-call": {
					const tool = o.tools[p.name];
					const call: ToolCall = { call: p.call, name: p.name, args: p.args, kwargs: p.kwargs };
					if (!tool) {
						p = session.resume({ kind: "error", error: { kind: "NameError", message: `no tool ${p.name}` } });
					} else if (o.sync?.has(p.name)) {
						try {
							p = session.resume({ kind: "value", value: (tool(call) ?? null) as HostValue });
						} catch (e) {
							p = session.resume({ kind: "error", error: hostError(e) });
						}
					} else {
						const entry: Open = { done: undefined, promise: Promise.resolve() };
						entry.promise = (async () => {
							try {
								entry.done = { ok: true, value: ((await tool(call)) ?? null) as HostValue };
							} catch (e) {
								entry.done = { ok: false, error: hostError(e) };
							}
						})();
						open.set(p.call, entry);
						p = session.resume({ kind: "future" });
					}
					break;
				}
				case "waiting": {
					const mine = p.calls.filter((c) => open.has(c));
					if (mine.length === 0) {
						const r = o.onStop?.(p);
						if (!r) return { kind: "stopped", progress: p, slices, fuel: fuel() };
						p = session.resume(r);
						break;
					}
					if (!mine.some((c) => open.get(c)?.done)) {
						await Promise.race(mine.map((c) => (open.get(c) as Open).promise));
					}
					const items: [number, Outcome][] = [];
					for (const c of mine) {
						const done = open.get(c)?.done;
						if (done) {
							items.push([c, done]);
							open.delete(c);
						}
					}
					p = session.resume({ kind: "batch", items });
					break;
				}
				case "parked": {
					if (p.reason !== "fuel" || slice === undefined) {
						const r = o.onStop?.(p);
						if (!r) return { kind: "stopped", progress: p, slices, fuel: fuel() };
						p = session.resume(r);
						break;
					}
					const used = fuel();
					const left = (o.fuelBudget ?? Number.POSITIVE_INFINITY) - used;
					const wanted = session.fuelWanted();
					if (left <= 0 || wanted > left) {
						session.cancel();
						return { kind: "budget", slices, fuel: used };
					}
					session.setFuel(Math.min(Math.max(slice, wanted), left));
					await (o.yieldEvery ?? macrotask)();
					slices++;
					p = session.resume({ kind: "continue" });
					break;
				}
				default:
					throw new MmError("abi", "unknown progress");
			}
		}
	} finally {
		if (ownTotal && !session.isDead) session.setFuelTotal(null);
		if (gcYield && !session.isDead) session.setGcYield(false);
	}
}
