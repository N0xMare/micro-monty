/**
 * The measurements themselves, shared by node.ts (Node) and worker/worker.ts
 * (workerd). Times are `performance.now()` deltas in ms. On deployed Workers
 * that clock stands still during synchronous execution; local workerd's does
 * not, which is what makes in-worker timing possible here.
 */
import {
	type Answer,
	hostcallScript,
	median,
	Mm,
	type MmSession,
	noopAnswer,
	noopTools,
	Pi,
	pyValues,
	Replay,
	type Task,
} from "./engines.ts";

export type Engine = { mm: Mm; pi?: undefined } | { pi: Pi; mm?: undefined };

/**
 * Records the memory every new wasm instance exports, so that linear memory can be
 * summed for either engine (pi's VMs are not reachable from outside). Returns the sum.
 */
export function trackMemories(): () => number {
	const memories: WeakRef<WebAssembly.Memory>[] = [];
	const track = (instance: WebAssembly.Instance) => {
		for (const v of Object.values(instance.exports)) if (v instanceof WebAssembly.Memory) memories.push(new WeakRef(v));
		return instance;
	};
	const Instance = WebAssembly.Instance;
	const Patched = function (this: unknown, m: WebAssembly.Module, i?: WebAssembly.Imports) {
		return track(new Instance(m, i));
	} as unknown as typeof WebAssembly.Instance;
	Patched.prototype = Instance.prototype;
	(WebAssembly as { Instance: unknown }).Instance = Patched;
	const instantiate = WebAssembly.instantiate.bind(WebAssembly) as (a: unknown, b?: unknown) => Promise<unknown>;
	(WebAssembly as { instantiate: unknown }).instantiate = async (a: unknown, b?: unknown) => {
		const r = (await instantiate(a, b)) as WebAssembly.Instance | WebAssembly.WebAssemblyInstantiatedSource;
		track(r instanceof WebAssembly.Instance ? r : r.instance);
		return r;
	};
	return () => memories.reduce((n, w) => n + (w.deref()?.buffer.byteLength ?? 0), 0);
}

const ms = (t: number) => performance.now() - t;

/** New session, codemode setup and a first cell, `reps + 1` times; the first is the cold one. */
export async function startup(e: Engine, tools: Task["tools"], reps: number) {
	const runs: { session: number; setup: number; first: number; total: number }[] = [];
	for (let i = 0; i <= reps; i++) {
		const t0 = performance.now();
		if (e.mm) {
			let t = performance.now();
			const m = e.mm.session();
			const session = ms(t);
			t = performance.now();
			e.mm.setup(m, tools);
			const setup = ms(t);
			t = performance.now();
			const r = await e.mm.run(m, "print(1)\n", noopAnswer);
			if (r.error || r.output !== "1\n") throw new Error(JSON.stringify(r));
			const first = ms(t);
			m.s.dispose();
			runs.push({ session, setup, first, total: ms(t0) });
		} else {
			const r = await e.pi.run(tools, "console.log(1);", noopAnswer);
			if (r.error || r.output !== "1\n") throw new Error(JSON.stringify(r));
			runs.push({ session: Number.NaN, setup: Number.NaN, first: Number.NaN, total: ms(t0) });
		}
	}
	const warm = runs.slice(1);
	const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;
	return {
		cold: runs[0],
		warm_totals: warm.map((w) => w.total),
		// Means as well: workerd's clock ticks in whole milliseconds.
		warm_total_mean_ms: mean(warm.map((w) => w.total)),
		warm_setup_mean_ms: mean(warm.map((w) => w.setup)),
		warm_session_mean_ms: mean(warm.map((w) => w.session)),
		warm_session_ms: median(warm.map((w) => w.session)),
		warm_setup_ms: median(warm.map((w) => w.setup)),
		warm_first_cell_ms: median(warm.map((w) => w.first)),
		warm_total_ms: median(warm.map((w) => w.total)),
	};
}

/** Starts one live session after setup (mm: a first cell ran; pi: a VM blocked mid-await) and keeps it in `live`. */
export async function liveSession(e: Engine, tools: Task["tools"], live: unknown[]): Promise<void> {
	if (e.mm) {
		const m = e.mm.session();
		e.mm.setup(m, tools);
		const r = await e.mm.run(m, "print(1)\n", noopAnswer);
		if (r.error) throw new Error(r.error);
		live.push(m);
		return;
	}
	const run = e.pi.start(e.pi.sandbox(), tools, "await tools.get_order({ order_id: 1 });", () => undefined);
	await new Promise((r) => setTimeout(r, 0));
	live.push(run);
}

export async function hostcall(e: Engine, kind: "await" | "fanout", n: number, reps: number) {
	const tool: number[] = [];
	const local: number[] = [];
	const lang = e.mm ? "py" : "js";
	const once = async (src: string) => {
		if (e.mm) {
			const m = e.mm.session();
			e.mm.setup(m, noopTools);
			const r = await e.mm.run(m, src, noopAnswer);
			if (r.error) throw new Error(r.error);
			m.s.dispose();
		} else {
			const r = await e.pi.run(noopTools, src, noopAnswer);
			if (r.error) throw new Error(r.error);
		}
	};
	for (let i = 0; i <= reps; i++) {
		for (const [src, out] of [
			[hostcallScript(lang, kind, n, false), tool],
			[hostcallScript(lang, kind, n, true), local],
		] as const) {
			const t = performance.now();
			await once(src);
			if (i > 0) out.push(ms(t));
		}
	}
	const calls = kind === "fanout" ? n * 100 : n;
	return { kind, n, tool_ms: tool, local_ms: local, per_call_us: ((median(tool) - median(local)) * 1000) / calls };
}

/** Each task end to end (new session, setup, script), `reps` times after one warm-up whose calls are returned for check.py. */
export async function workload(e: Engine, tasks: Task[], reps: number) {
	const results = [];
	for (const task of tasks) {
		const values = e.mm ? pyValues(task) : task.calls.map((c) => c.ok);
		const total: number[] = [];
		const setup: number[] = [];
		let check: Record<string, unknown> = {};
		let linear = 0;
		for (let i = 0; i <= reps; i++) {
			const replay = new Replay(task, values);
			const answer = (n: string, a: unknown): Answer => replay.call(n, a);
			const t = performance.now();
			let r;
			let s = Number.NaN;
			if (e.mm) {
				const m = e.mm.session();
				e.mm.setup(m, task.tools);
				s = ms(t);
				r = await e.mm.run(m, task.py, answer);
				linear = m.s.memoryBytes();
				m.s.dispose();
			} else {
				r = await e.pi.run(task.tools, task.js, answer);
			}
			const all = ms(t);
			if (i === 0) check = { error: r.error ?? null, misses: replay.misses, calls: replay.calls };
			else {
				total.push(all);
				setup.push(s);
			}
		}
		results.push({ task: task.task, total_ms: median(total), setup_ms: median(setup), linear: linear || null, check });
	}
	return { reps, tasks: results, sum_total_ms: results.reduce((n, r) => n + r.total_ms, 0) };
}

export const SNAP_POINTS = [
	["neg-fanout-40", 0],
	["agg-avg-order-by-tier", 3],
	["par-two-level-fanout", 1],
] as const;

/** mm's mid-await image at `name@after`: size, snapshot and restore times, and a finished restore. */
export function mmSnapshot(mm: Mm, task: Task, after: number) {
	const values = pyValues(task);
	const m = mm.session();
	mm.setup(m, task.tools);
	const replay = new Replay(task, values);
	const stop = mm.runUntil(m, task.py, replay, after);
	if (!stop) throw new Error(`${task.task} did not wait`);
	const snapMs: number[] = [];
	let image: Uint8Array = new Uint8Array();
	for (let i = 0; i < 10; i++) {
		const t = performance.now();
		image = m.s.snapshot();
		snapMs.push(ms(t));
	}
	const restoreMs: number[] = [];
	let finished: unknown;
	for (let i = 0; i < 10; i++) {
		const t = performance.now();
		const s = mm.rt.restore(image, mm.options());
		restoreMs.push(ms(t));
		if (i === 0) {
			const r = { s, out: [] as string[], params: m.params };
			s.setOutput((_, x) => {
				r.out.push(x);
			});
			// The answered calls first, then the open ones, as the replay saw them.
			const rest = new Replay(task, values);
			for (const [n, a] of replay.calls.slice(0, replay.calls.length - stop.open.length)) rest.call(n, a);
			const res = mm.finish(r, stop.open, rest);
			finished = { ok: !res.error, error: res.error ?? null, calls_after_restore: rest.calls.length - (replay.calls.length - stop.open.length) };
		}
		s.dispose();
	}
	const linear = m.s.memoryBytes();
	m.s.dispose();
	return { point: `${task.task}@${after}`, image, snapshot_ms: median(snapMs), restore_ms: median(restoreMs), linear, finished };
}

/**
 * One program's CPU per fuel unit: `setup` runs unmetered, then `op` runs in slices of
 * `slice` fuel, as a Workers host would run it (`fuelWanted()` granted when a native
 * asks for more). `granted` is the fuel the host gave out, less what was left, which
 * is what a budget counts; `charged` is what the engine charged, taken from a session
 * fuel total, which also counts what natives that charge after they run overdrew.
 * The longest slice is the longest the isolate's event loop was blocked.
 */
export function fuelRate(mm: Mm, setup: string, op: string, slice = 10_000) {
	const s = mm.rt.session(mm.options());
	s.setOutput(() => {});
	try {
		const ps = s.start(setup, "setup.py");
		if (ps.kind !== "complete") throw new Error(`setup: ${JSON.stringify(ps)}`);
		const TOTAL = 1e15;
		s.setFuelTotal(TOTAL);
		s.setFuel(slice);
		let granted = slice;
		let used = 0;
		let slices = 1;
		let maxSlice = 0;
		let maxGrant = slice;
		// The most one slice charged past what it was given (the overdraft).
		let maxOver = 0;
		let totalBefore = TOTAL;
		const over = () => {
			const now = s.fuelTotal() ?? TOTAL;
			maxOver = Math.max(maxOver, totalBefore - now - granted);
			totalBefore = now;
		};
		const t0 = performance.now();
		let ts = t0;
		let p = s.start(op, "op.py");
		while (p.kind === "parked" && p.reason === "fuel") {
			maxSlice = Math.max(maxSlice, performance.now() - ts);
			over();
			used += granted - (s.fuel() ?? 0);
			granted = Math.max(slice, s.fuelWanted());
			maxGrant = Math.max(maxGrant, granted);
			s.setFuel(granted);
			slices++;
			ts = performance.now();
			p = s.resume({ kind: "continue" });
		}
		const end = performance.now();
		maxSlice = Math.max(maxSlice, end - ts);
		if (p.kind !== "complete") throw new Error(`op: ${JSON.stringify(p).slice(0, 400)}`);
		over();
		used += granted - (s.fuel() ?? 0);
		const charged = TOTAL - (s.fuelTotal() ?? TOTAL);
		const total = end - t0;
		return {
			ms: total,
			granted: used,
			charged,
			us_per_granted: (total * 1000) / used,
			us_per_charged: (total * 1000) / charged,
			slices,
			max_slice_ms: maxSlice,
			max_grant: maxGrant,
			max_overdraft: maxOver,
		};
	} finally {
		s.dispose();
	}
}

/**
 * Sites of long native work, for {@link fuelWorst}. `{n}` is the size; `search` probes
 * grow it to the largest the session's memory limit admits (the longest one native
 * step can then be), the others run at `n` to measure their rate. `wanted` marks
 * operations that park first with `fuelWanted()`, which a budget refuses.
 */
export const FUEL_PROBES: { name: string; setup: string; op: string; n: number; search: boolean; wanted?: boolean }[] = [
	{ name: "str.translate", setup: "s = 'abc' * {n}\nt = str.maketrans('abc', 'xyz')", op: "u = s.translate(t)", n: 50_000, search: true },
	{ name: "str.upper", setup: "s = 'ab' * {n}", op: "t = s.upper()", n: 50_000, search: true },
	{ name: "str.replace", setup: "s = 'ab' * {n}", op: "t = s.replace('a', 'xy')", n: 50_000, search: true },
	{ name: "str.count", setup: "s = 'ab' * {n}", op: "c = s.count('b')", n: 50_000, search: true },
	{ name: "str.find, no match", setup: "s = 'a' * {n}", op: "i = s.find('ab')", n: 100_000, search: true },
	{ name: "bytes.find, no match", setup: "s = b'a' * {n}", op: "i = s.find(b'a' * 1000 + b'b')", n: 100_000, search: true },
	{ name: "int(str), invalid", setup: "s = '1' * {n} + 'x'", op: "try:\n    int(s)\nexcept ValueError:\n    pass", n: 100_000, search: true },
	{ name: "hashlib.sha256", setup: "import hashlib\nb = b'x' * {n}", op: "h = hashlib.sha256(b).hexdigest()", n: 100_000, search: true },
	{ name: "bytes.hex", setup: "b = b'x' * {n}", op: "h = b.hex()", n: 100_000, search: true },
	{ name: "bytes.decode", setup: "b = 'é'.encode() * {n}", op: "s = b.decode()", n: 50_000, search: true },
	{ name: "json.loads", setup: "import json\ns = json.dumps(list(range({n})))", op: "xs = json.loads(s)", n: 20_000, search: true },
	{ name: "dict(zip(it, it))", setup: "it = iter(range({n}))", op: "d = dict(zip(it, it))", n: 20_000, search: true },
	{ name: "set & set", setup: "a = set(range({n}))\nb = set(range({n} // 2, {n} * 3 // 2))", op: "c = a & b", n: 20_000, search: true },
	{ name: "dict(d)", setup: "d = {i: i for i in range({n})}", op: "e = dict(d)", n: 20_000, search: true },
	{ name: "max(list)", setup: "xs = list(range({n}))", op: "x = max(xs)", n: 20_000, search: true },
	{ name: "hash(tuple)", setup: "t = tuple(range({n}))", op: "h = hash(t)", n: 20_000, search: true },
	{ name: "str(list), repr", setup: "xs = [[i] for i in range({n})]", op: "s = str(xs)", n: 20_000, search: true },
	{ name: "gc.collect()", setup: "import gc\nxs = [[i] for i in range({n})]", op: "gc.collect()", n: 20_000, search: true },
	{ name: "sorted(list of str)", setup: "xs = [str((i * 7919) % 1_000_003) for i in range({n})]", op: "ys = sorted(xs)", n: 20_000, search: true, wanted: true },
	{ name: "math.gcd", setup: "import math\nx = 7 ** {n} + 1\ny = 3 ** ({n} * 5 // 3)", op: "z = math.gcd(x, y)", n: 60_000, search: false, wanted: true },
	{ name: "divmod, big ints", setup: "x = 7 ** ({n} * 5)\ny = 3 ** ({n} * 3)", op: "q, r = divmod(x, y)", n: 60_000, search: false, wanted: true },
	{ name: "math.isqrt", setup: "import math\nx = 7 ** {n}", op: "r = math.isqrt(x)", n: 60_000, search: false, wanted: true },
	{ name: "pow(b, e, m)", setup: "m = 7 ** 3000 + 1\nb = 3 ** 2000\ne = 7 ** {n}", op: "z = pow(b, e, m)", n: 1_500, search: false, wanted: true },
	{ name: "re, backtracking", setup: "import re\ns = 'a' * {n}", op: "m = re.match(r'(a|a)*c', s)", n: 20, search: false },
	{ name: "re.search, no match", setup: "import re\ns = 'a' * {n}", op: "m = re.search(r'a*b', s)", n: 6_000, search: false },
	{ name: "list of lists, a loop", setup: "", op: "xs = []\nfor i in range({n}):\n    xs.append([i])", n: 100_000, search: false },
];

/**
 * One {@link FUEL_PROBES} site under a `memory` limit: at `n`, or for `search` sites
 * at the largest size that runs (doubling until a `MemoryError`, then bisecting),
 * with a run over `capMs` ending the growth.
 */
export function fuelWorst(module: WebAssembly.Module, memory: number, probe: (typeof FUEL_PROBES)[number], capMs = 1500) {
	const mm = new Mm(module, { memory });
	const at = (n: number) => {
		try {
			return fuelRate(mm, probe.setup.replaceAll("{n}", String(n)), probe.op.replaceAll("{n}", String(n)));
		} catch {
			return undefined;
		}
	};
	let n = probe.n;
	let best = at(n);
	if (!best) return { name: probe.name, memory, error: "fails at its smallest size" };
	if (probe.search) {
		let bad = 0;
		while (best.ms < capMs) {
			const r = at(n * 2);
			if (!r) {
				bad = n * 2;
				break;
			}
			n *= 2;
			best = r;
		}
		for (let i = 0; bad && i < 5 && bad - n > n / 32; i++) {
			const mid = Math.floor((n + bad) / 2);
			const r = at(mid);
			if (r) [n, best] = [mid, r];
			else bad = mid;
		}
	}
	return { name: probe.name, memory, n, wanted: probe.wanted ?? false, ...best };
}

/**
 * The bench programs' rates: each run whole in 10,000-fuel slices, after one warm-up.
 * `fuel` is what the engine charged; `granted` what the slices gave, which misses
 * overdrafts.
 */
export function fuelPrograms(module: WebAssembly.Module, programs: { name: string; py: string; expected: string }[], reps: number) {
	const mm = new Mm(module);
	return programs.map((p) => {
		const runs = [];
		for (let i = 0; i <= reps; i++) {
			const r = fuelRate(mm, "", p.py);
			if (i > 0) runs.push(r);
		}
		const check = checkOutput(mm, p.py, p.expected);
		return {
			name: p.name,
			ms: median(runs.map((r) => r.ms)),
			fuel: median(runs.map((r) => r.charged)),
			us_per_fuel: median(runs.map((r) => r.us_per_charged)),
			granted: median(runs.map((r) => r.granted)),
			us_per_granted: median(runs.map((r) => r.us_per_granted)),
			max_slice_ms: Math.max(...runs.map((r) => r.max_slice_ms)),
			max_overdraft: Math.max(...runs.map((r) => r.max_overdraft)),
			ok: check,
		};
	});
}

function checkOutput(mm: Mm, py: string, expected: string): boolean {
	const s = mm.rt.session(mm.options());
	let out = "";
	s.setOutput((_, t) => {
		out += t;
	});
	const p = s.start(py, "check.py");
	s.dispose();
	return p.kind === "complete" && out.trim() === expected;
}

/** The 22 tasks' rates: each cell through the driver in 10,000-fuel slices, the run timed without session and setup. */
export async function fuelTasks(module: WebAssembly.Module, tasks: Task[], reps: number) {
	const mm = new Mm(module, { fuelSlice: 10_000, yieldEvery: () => Promise.resolve() });
	const out = [];
	for (const task of tasks) {
		const values = pyValues(task);
		const runs: { ms: number; fuel: number }[] = [];
		let error: string | undefined;
		for (let i = 0; i <= reps; i++) {
			const replay = new Replay(task, values);
			const m = mm.session();
			mm.setup(m, task.tools);
			const t = performance.now();
			const r = await mm.run(m, task.py, (n, a) => replay.call(n, a));
			const dt = ms(t);
			m.s.dispose();
			if (r.error) error = r.error.slice(-200);
			if (i > 0) runs.push({ ms: dt, fuel: r.fuel ?? 0 });
		}
		const msMed = median(runs.map((r) => r.ms));
		const fuel = median(runs.map((r) => r.fuel));
		out.push({ task: task.task, ms: msMed, fuel, us_per_fuel: (msMed * 1000) / fuel, error: error ?? null });
	}
	const sumMs = out.reduce((n, r) => n + r.ms, 0);
	const sumFuel = out.reduce((n, r) => n + r.fuel, 0);
	return { tasks: out, sum_ms: sumMs, sum_fuel: sumFuel, us_per_fuel: (sumMs * 1000) / sumFuel };
}

/**
 * K sessions per instance: `n` live sessions on a runtime with `sessionsPerInstance`
 * `k`, bare or set up (codemode setup and a first cell, as {@link liveSession}).
 * Linear memory is the runtime's total; creation times are means, split by whether
 * the session started a new instance or took a slot in a live one.
 */
export async function kSessions(module: WebAssembly.Module, k: number, n: number, setup: boolean, tools: Task["tools"]) {
	const mm = new Mm(module, { perInstance: k });
	const live: MmSession[] = [];
	const fresh: number[] = [];
	const shared: number[] = [];
	const t0 = performance.now();
	for (let i = 0; i < n; i++) {
		const before = mm.rt.stats().instances;
		const t = performance.now();
		const m = mm.session();
		if (setup) {
			mm.setup(m, tools);
			const r = await mm.run(m, "print(1)\n", noopAnswer);
			if (r.error) throw new Error(r.error);
		}
		(mm.rt.stats().instances > before ? fresh : shared).push(ms(t));
		live.push(m);
	}
	const total = ms(t0);
	const st = mm.rt.stats();
	const mean = (xs: number[]) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null);
	return {
		k,
		n,
		setup,
		instances: st.instances,
		linear_total: st.memoryBytes,
		linear_per_session: st.memoryBytes / n,
		create_mean_ms: total / n,
		new_instance_mean_ms: mean(fresh),
		shared_slot_mean_ms: mean(shared),
		live,
	};
}
