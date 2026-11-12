/**
 * The measured engines behind one interface, shared by the Node runner
 * (node.ts) and the workerd Worker (worker/worker.ts):
 *
 * - `mm`: the mm-wasm guest on this package's host (../src/host.ts), with the
 *   codemode setup of bench/evidence/engines/hosts/mm-wasm: one builtins module
 *   with each tool as a typed function (keyword-only parameters defaulting to
 *   `None`, `dict_as_kwargs`), answered through ../src/driver.ts (awaited calls
 *   and fan-outs as futures, answered in batches).
 * - `pi`: pi's codemode exactly as it runs on Cloudflare: its
 *   `InlineCodemodeSandbox` (one quickjs-wasi 3.6.2 VM per script, pi's prelude,
 *   the VM on the caller's thread) driven through `start()`, the host-controlled
 *   run whose `deliver()` carries an error's `type` as the harness's JS
 *   solutions expect (`e.type`). The sandbox class is passed in, so this file
 *   does not import pi.
 *
 * Both answer tool calls from the recorded fixtures (bench/evidence/engines/fixtures)
 * keyed as the Rust harness keys them (hosts/common `key`).
 */

import { run as drive } from "../src/driver.ts";
import {
	Float,
	type HostModule,
	type HostSignature,
	type HostValue,
	type Outcome,
	type Progress,
	type Resume,
	Runtime,
	type Session,
	sessionOptions,
	toJs,
} from "../src/host.ts";

export interface ToolDef {
	name: string;
	description: string;
	params: string[];
}

interface Call {
	name: string;
	args: unknown;
	ok?: unknown;
	err?: [string, string];
}

export interface Task {
	task: string;
	tools: ToolDef[];
	calls: Call[];
	/** The fixture's text, parsed again for mm so that `2.0` stays a float. */
	text: string;
	py: string;
	js: string;
}

export type Answer = { ok: true; value: unknown } | { ok: false; type: string; message: string };

/** `name` and canonical arguments, as hosts/common's `key`: sorted keys, integral numbers as integers. */
export function key(name: string, args: unknown): string {
	return `${name} ${canon(args)}`;
}

function canon(v: unknown): string {
	if (Array.isArray(v)) return `[${v.map(canon).join(",")}]`;
	if (v && typeof v === "object") {
		const o = v as Record<string, unknown>;
		return `{${Object.keys(o)
			.sort()
			.map((k) => `${JSON.stringify(k)}:${canon(o[k])}`)
			.join(",")}}`;
	}
	if (typeof v === "bigint") return v.toString();
	return JSON.stringify(v ?? null);
}

/** Answers a run's calls from a fixture; repeated identical calls get the recorded replies in order. */
export class Replay {
	private table = new Map<string, number[]>();
	readonly calls: [string, unknown][] = [];
	readonly misses: string[] = [];
	private readonly replies: Answer[];
	constructor(task: Task, values: unknown[]) {
		this.replies = task.calls.map((c, i) =>
			c.err ? { ok: false, type: c.err[0], message: c.err[1] } : { ok: true, value: values[i] },
		);
		task.calls.forEach((c, i) => {
			const k = key(c.name, c.args);
			const list = this.table.get(k);
			if (list) list.push(i);
			else this.table.set(k, [i]);
		});
	}
	call(name: string, args: unknown): Answer {
		this.calls.push([name, args]);
		const k = key(name, args);
		const i = this.table.get(k)?.shift();
		if (i !== undefined) return this.replies[i] as Answer;
		if (name === "submit_answer") return { ok: true, value: "ok" };
		this.misses.push(k);
		return { ok: false, type: "RuntimeError", message: "no recorded reply for this call" };
	}
}

/**
 * The fixture's replies with Python's types: a number written with a fraction or an
 * exponent is a `Float` (JSON.parse's source access), as serde_json keeps it in the
 * Rust harness.
 */
export function pyValues(task: Task): unknown[] {
	type Ctx = { source?: string };
	const parsed = JSON.parse(task.text, function (this: unknown, _k: string, v: unknown, ctx?: Ctx) {
		if (typeof v === "number" && Number.isInteger(v) && ctx?.source && /[.eE]/.test(ctx.source)) return new Float(v);
		return v;
	} as (this: unknown, k: string, v: unknown) => unknown) as { calls: Call[] };
	return parsed.calls.map((c) => c.ok);
}

export interface RunOutcome {
	output: string;
	error?: string;
	/** mm with `fuelSlice`: the fuel the cell used. */
	fuel?: number;
}

// ---------------------------------------------------------------------- mm

export interface MmOptions {
	/** Arena limit per session (`Limits::memory`). */
	memory?: number;
	/** Run cells in fuel slices of this size (yielding between them). */
	fuelSlice?: number;
	/** `RuntimeConfig.sessionsPerInstance` (K); default 1. */
	perInstance?: number;
	/** The driver's yield between fuel slices (default a macrotask). */
	yieldEvery?: () => Promise<void>;
}

export interface MmSession {
	s: Session;
	out: string[];
	params: Map<string, string[]>;
}

export function mmModule(tools: ToolDef[]): HostModule {
	const functions: HostSignature[] = tools.map((t) => ({
		name: t.name,
		params: t.params.map((p) => ({ name: p, default: null, kind: "keyword-only" as const })),
		doc: t.description,
		dictAsKwargs: true,
	}));
	functions.push({
		name: "hsync",
		params: [
			{ name: "args", kind: "var-positional" },
			{ name: "kwargs", kind: "var-keyword" },
		],
	});
	return { name: "", functions };
}

/** The argument dict a typed tool's call stands for: its parameters, without the `None` ones. */
function toolArgs(params: string[] | undefined, args: HostValue[]): Record<string, unknown> {
	const o: Record<string, unknown> = {};
	params?.forEach((p, i) => {
		const v = args[i];
		if (v !== null && v !== undefined) o[p] = toJs(v);
	});
	return o;
}

export class Mm {
	readonly rt: Runtime;
	readonly o: MmOptions;
	constructor(module: WebAssembly.Module, o: MmOptions = {}) {
		this.rt = Runtime.fromModule(module, o.perInstance ? { sessionsPerInstance: o.perInstance } : {});
		this.o = o;
	}

	options() {
		return sessionOptions(this.o.memory ? { limits: { memory: this.o.memory } } : {});
	}

	session(): MmSession {
		const s = this.rt.session(this.options());
		const out: string[] = [];
		s.setOutput((_, t) => {
			out.push(t);
		});
		return { s, out, params: new Map() };
	}

	setup(m: MmSession, tools: ToolDef[]): void {
		m.s.define(mmModule(tools));
		for (const t of tools) m.params.set(t.name, t.params);
	}

	/** A cell through the async driver; tools answer from `answer` (a replay, or a no-op). */
	async run(m: MmSession, src: string, answer: (name: string, args: unknown) => Answer): Promise<RunOutcome> {
		m.out.length = 0;
		const tools: Record<string, (c: { name: string; args: HostValue[] }) => unknown> = {};
		for (const name of m.params.keys()) {
			tools[name] = async (c) => {
				const a = answer(c.name, toolArgs(m.params.get(c.name), c.args));
				if (a.ok) return a.value;
				throw { kind: a.type, message: a.message };
			};
		}
		tools.hsync = (c) => c.args[0] ?? null;
		const r = await drive(m.s, src, {
			tools,
			sync: new Set(["hsync"]),
			...(this.o.fuelSlice ? { fuelSlice: this.o.fuelSlice } : {}),
			...(this.o.yieldEvery ? { yieldEvery: this.o.yieldEvery } : {}),
		});
		const output = m.out.join("");
		const fuel = r.fuel;
		if (r.kind === "complete") return { output, fuel };
		if (r.kind === "error") return { output, error: r.progress.error.traceback, fuel };
		return { output, error: r.kind, fuel };
	}

	/**
	 * The harness's `run_until`: answers every call as it is issued (a future) and
	 * delivers the answers when the cell waits, until `after` were delivered and the
	 * cell waits again. Returns the open calls, which a host keeps beside the image.
	 */
	runUntil(m: MmSession, src: string, replay: Replay, after: number): { open: [number, string, unknown][] } | undefined {
		const open: [number, string, unknown, Answer][] = [];
		let answered = 0;
		let p: Progress = m.s.start(src, "cell.py");
		for (;;) {
			if (p.kind === "host-call") {
				const args = toolArgs(m.params.get(p.name), p.args);
				open.push([p.call, p.name, args, replay.call(p.name, args)]);
				p = m.s.resume({ kind: "future" });
			} else if (p.kind === "waiting") {
				if (answered >= after) return { open: open.map(([c, n, a]) => [c, n, a]) };
				const r = batch(open, p.calls);
				answered += r.items.length;
				p = m.s.resume(r);
			} else return undefined;
		}
	}

	/** Finishes a restored session at `pending()`, answering its open calls and the rest from `replay`. */
	finish(m: MmSession, open: [number, string, unknown][], replay: Replay): RunOutcome {
		const pend: [number, string, unknown, Answer][] = open.map(([c, n, a]) => [c, n, a, replay.call(n, a)]);
		let p = m.s.pending();
		for (;;) {
			if (!p) return { output: m.out.join(""), error: "not stopped" };
			if (p.kind === "host-call") {
				const args = toolArgs(m.params.get(p.name), p.args);
				pend.push([p.call, p.name, args, replay.call(p.name, args)]);
				p = m.s.resume({ kind: "future" });
			} else if (p.kind === "waiting") p = m.s.resume(batch(pend, p.calls));
			else if (p.kind === "complete") return { output: m.out.join("") };
			else return { output: m.out.join(""), error: JSON.stringify(p) };
		}
	}
}

function batch(open: [number, string, unknown, Answer][], calls: number[]): Extract<Resume, { kind: "batch" }> {
	const items: [number, Outcome][] = [];
	for (let i = open.length - 1; i >= 0; i--) {
		const o = open[i] as [number, string, unknown, Answer];
		if (!calls.includes(o[0])) continue;
		const a = o[3];
		items.unshift([
			o[0],
			a.ok ? { ok: true, value: a.value as HostValue } : { ok: false, error: { kind: a.type, message: a.message } },
		]);
		open.splice(i, 1);
	}
	return { kind: "batch", items };
}

// ---------------------------------------------------------------------- pi

/** The parts of pi's `CodemodeSandboxBase.start()` this harness uses. */
export interface PiRun {
	readonly result: Promise<{ ok: boolean; output: { type: string; text?: string }[]; error?: { message: string; stack?: string } }>;
	deliver(seq: number, now: number, id: number, outcome: unknown): void;
}
export interface PiSandbox {
	start(code: string, options: Record<string, unknown>): PiRun;
}
export type PiSandboxClass = new (options: Record<string, unknown>) => PiSandbox;

export class Pi {
	readonly Sandbox: PiSandboxClass;
	readonly module: WebAssembly.Module;
	readonly memoryLimitBytes: number | undefined;
	constructor(Sandbox: PiSandboxClass, module: WebAssembly.Module, memoryLimitBytes?: number) {
		this.Sandbox = Sandbox;
		this.module = module;
		this.memoryLimitBytes = memoryLimitBytes;
	}

	sandbox(): PiSandbox {
		return new this.Sandbox({
			wasm: this.module,
			timeoutMs: Number.POSITIVE_INFINITY,
			...(this.memoryLimitBytes ? { memoryLimitBytes: this.memoryLimitBytes } : {}),
		});
	}

	/**
	 * Starts `code` in a new VM. `answer` returns the call's answer now, or `undefined`
	 * to leave it open (a live session blocked mid-await).
	 */
	start(sb: PiSandbox, tools: ToolDef[], code: string, answer: (name: string, args: unknown) => Answer | undefined): PiRun {
		let seq = 1;
		const run: PiRun = sb.start(code, {
			startedAt: Date.now(),
			seed: 1,
			tools: tools.map((t) => ({ name: t.name, description: t.description })),
			onCall(c: { id: number; name: string; args: unknown }) {
				const a = answer(c.name, c.args);
				if (!a) return;
				run.deliver(
					seq++,
					Date.now(),
					c.id,
					a.ok ? { ok: true, value: a.value } : { ok: false, message: a.message, properties: { type: a.type } },
				);
			},
		});
		return run;
	}

	async run(tools: ToolDef[], code: string, answer: (name: string, args: unknown) => Answer): Promise<RunOutcome> {
		const r = await this.start(this.sandbox(), tools, code, answer).result;
		const output = r.output.map((o) => (o.text ?? "") + "\n").join("");
		return r.ok ? { output } : { output, error: r.error?.stack ?? r.error?.message ?? "failed" };
	}
}

export const noopTools: ToolDef[] = [{ name: "noop", description: "Does nothing.", params: ["i"] }];
export const noopAnswer = (): Answer => ({ ok: true, value: { ok: true } });

/** The host-call scripts of hosts/common (`await_script`, `fanout_script`). */
export function hostcallScript(lang: "py" | "js", kind: "await" | "fanout", n: number, local: boolean): string {
	if (lang === "js") {
		if (kind === "await")
			return local
				? `const f = async (a) => a; for (let i = 0; i < ${n}; i++) await f({ i });`
				: `for (let i = 0; i < ${n}; i++) await tools.noop({ i });`;
		return local
			? `const f = async (a) => a; for (let r = 0; r < ${n}; r++) await Promise.all(Array.from({ length: 100 }, (_, i) => f({ i })));`
			: `for (let r = 0; r < ${n}; r++) await Promise.all(Array.from({ length: 100 }, (_, i) => tools.noop({ i })));`;
	}
	if (kind === "await")
		return local
			? `async def f(a):\n    return a\nfor i in range(${n}):\n    await f({'i': i})\n`
			: `for i in range(${n}):\n    await noop({'i': i})\n`;
	return local
		? `import asyncio\nasync def f(a):\n    return a\nfor r in range(${n}):\n    await asyncio.gather(*[f({'i': i}) for i in range(100)])\n`
		: `import asyncio\nfor r in range(${n}):\n    await asyncio.gather(*[noop({'i': i}) for i in range(100)])\n`;
}

export function median(xs: number[]): number {
	const v = [...xs].sort((a, b) => a - b);
	const n = v.length;
	if (n === 0) return Number.NaN;
	return n % 2 ? (v[(n - 1) / 2] as number) : ((v[n / 2 - 1] as number) + (v[n / 2] as number)) / 2;
}
