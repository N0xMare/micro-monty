/**
 * The Node (V8) side of the measurements. One command per process, one JSON line
 * on stdout; run.py runs them in rounds, pinned to one core.
 *
 *   node --expose-gc bench/node.ts CMD ENGINE [ARGS]
 *
 * ENGINE is `mm:<variant>` (a guest in hosts/js/guests: default, core35, nostdlib)
 * or `pi` (pi's InlineCodemodeSandbox on quickjs-wasi 3.6.2).
 *
 * - `startup ENGINE REPS`: the cold path of a fresh process (compile, first
 *   instance, first session, codemode setup, first cell), then REPS warm sessions.
 * - `mem ENGINE N`: N live set-up sessions; RSS, V8 heap and wasm linear memory added.
 * - `hostcall ENGINE await|fanout N REPS`: N awaited calls (or N fan-outs of 100) of a
 *   no-op tool against the same of a local async function, per call.
 * - `workload ENGINE REPS`: the 22 tasks end to end (new session, setup, script),
 *   REPS times each after one warm-up; the first run's calls go to check.py.
 * - `memcap ENGINE MIB`: a session limited to MIB (mm: `limits.memory`; pi:
 *   `memoryLimitBytes`) runs a cell that allocates until it fails; linear memory
 *   before, after, and after a second cell in the same session.
 * - `snapshot ENGINE`: mm's mid-await images at the harness's three points; for pi,
 *   quickjs-wasi's own snapshot of a VM blocked at the same first point.
 * - `ksessions ENGINE K N bare|setup`: N live sessions with K sessions per instance;
 *   linear memory, RSS and creation time per session (mm only).
 * - `fuel ENGINE programs|tasks REPS`: µs per fuel unit of the bench programs, or of
 *   the 22 tasks through the driver, in 10,000-fuel slices (mm only).
 * - `fuel ENGINE worst MIB`: FUEL_PROBES under a MIB limit: the longest slice, the
 *   overdraft and µs per fuel unit of long native work (mm only).
 */
import { readFileSync } from "node:fs";
import { gzipSync, zstdCompressSync } from "node:zlib";
import { loadPrograms, loadTasks, guestPath, piCodemode, quickjsPath } from "./data.ts";
import { median, Mm, Pi, type PiSandboxClass, type Task } from "./engines.ts";
import {
	type Engine,
	FUEL_PROBES,
	fuelPrograms,
	fuelTasks,
	fuelWorst,
	hostcall,
	kSessions,
	liveSession,
	mmSnapshot,
	SNAP_POINTS,
	startup,
	trackMemories,
	workload,
} from "./measure.ts";

const linear = trackMemories();

const [cmd = "", engine = "", ...rest] = process.argv.slice(2);
const num = (i: number, d: number) => (rest[i] ? Number(rest[i]) : d);
const ms = (t: number) => performance.now() - t;
const gc = (globalThis as { gc?: () => void }).gc ?? (() => {});
const emit = (o: Record<string, unknown>) =>
	process.stdout.write(`${JSON.stringify({ cmd, engine, node: process.version, ...o })}\n`);

const isMm = engine.startsWith("mm:");
const variant = engine.slice(3);
const bytes = readFileSync(isMm ? guestPath(variant) : quickjsPath());
const tasks = loadTasks();
const byName = (n: string) => tasks.find((t) => t.task === n) as Task;

async function piClass(): Promise<PiSandboxClass> {
	const m = await import(`${piCodemode()}/src/portable.ts`);
	return m.InlineCodemodeSandbox as PiSandboxClass;
}

const fixture = byName("neg-fanout-40");

// pi's TypeScript is loaded before anything is timed.
const PiClass = isMm ? undefined : await piClass();
async function engineOf(module: WebAssembly.Module): Promise<Engine> {
	return isMm ? { mm: new Mm(module) } : { pi: new Pi(PiClass as PiSandboxClass, module) };
}

if (cmd === "startup") {
	// The cold path of this fresh process: compile, the first instance (mm's runtime
	// probes one for the build id), then the first and the warm sessions.
	const reps = num(0, 20);
	// Synchronous, as workerd compiles a Worker's wasm modules when it loads them (an
	// async compile under --no-wasm-lazy-compilation also leaves Node's loop idle).
	const rss = () => process.memoryUsage().rss;
	const rss_before = rss();
	let t = performance.now();
	const module = new WebAssembly.Module(bytes);
	const compile_ms = ms(t);
	const rss_compiled = rss();
	t = performance.now();
	const e = await engineOf(module);
	const runtime_ms = ms(t);
	const r = await startup(e, fixture.tools, 0);
	const rss_cold = rss();
	emit({
		compile_ms,
		runtime_ms,
		bytes: bytes.length,
		// The module's own cost: compiled (validated, lazily compiled stubs), then after
		// a first session compiled the functions it touched.
		rss_compile: rss_compiled - rss_before,
		rss_first_session: rss_cold - rss_compiled,
		...r,
		...(reps > 0 ? await startup(e, fixture.tools, reps) : {}),
		cold: r.cold,
	});
} else if (cmd === "mem") {
	const n = num(0, 40);
	const e = await engineOf(await WebAssembly.compile(bytes));
	const live: unknown[] = [];
	const settle = async () => {
		for (let i = 0; i < 3; i++) {
			gc();
			await new Promise((r) => setTimeout(r, 20));
		}
	};
	const sample = () => {
		const u = process.memoryUsage();
		return { rss: u.rss, heap: u.heapUsed, external: u.external, linear: linear() };
	};
	await liveSession(e, fixture.tools, live);
	live.length = 0;
	await settle();
	const before = sample();
	for (let i = 0; i < n; i++) await liveSession(e, fixture.tools, live);
	await settle();
	const after = sample();
	emit({
		n,
		before,
		after,
		per_session: Object.fromEntries(
			Object.entries(after).map(([k, v]) => [k, (v - before[k as keyof typeof before]) / n]),
		),
	});
} else if (cmd === "hostcall") {
	const e = await engineOf(await WebAssembly.compile(bytes));
	emit(await hostcall(e, rest[0] as "await" | "fanout", num(1, 2000), num(2, 5)));
} else if (cmd === "workload") {
	const e = await engineOf(await WebAssembly.compile(bytes));
	emit(await workload(e, tasks, num(0, 5)));
} else if (cmd === "memcap") {
	const mib = num(0, 32);
	const module = await WebAssembly.compile(bytes);
	const grow = { py: "xs = []\nwhile True:\n    xs.append('x' * 100000)\n", js: "const xs = []; while (true) xs.push('x'.repeat(100000) + xs.length);" };
	if (isMm) {
		const mm = new Mm(module, { memory: mib << 20 });
		const m = mm.session();
		mm.setup(m, fixture.tools);
		const before = m.s.memoryBytes();
		const r = await mm.run(m, grow.py, () => ({ ok: true, value: null }));
		const after = m.s.memoryBytes();
		const again = await mm.run(m, "xs = None\nprint(sum(range(10)))\n", () => ({ ok: true, value: null }));
		emit({ mib, before, after, raised: r.error?.split("\n").at(-2), again: again.output, after_again: m.s.memoryBytes() });
	} else {
		const pi = new Pi(PiClass as PiSandboxClass, module, mib << 20);
		const before = linear();
		const r = await pi.run(fixture.tools, grow.js, () => ({ ok: true, value: null }));
		emit({ mib, before, peak_linear_all_vms: linear(), raised: r.error?.split("\n")[0] });
	}
} else if (cmd === "snapshot") {
	const module = await WebAssembly.compile(bytes);
	const out = [];
	if (isMm) {
		const mm = new Mm(module);
		for (const [name, after] of SNAP_POINTS) {
			const r = mmSnapshot(mm, byName(name), after);
			out.push({
				...r,
				image: undefined,
				raw: r.image.length,
				zstd1: zstdCompressSync(r.image, { params: { 100: 1 } }).length,
				gzip6: gzipSync(r.image).length,
			});
		}
	} else {
		// quickjs-wasi's own snapshot (pi itself replays instead of snapshotting): a VM
		// with pi's prelude, blocked on neg-fanout-40's 40 calls, as vm.ts sets it up.
		const { QuickJS } = await import("quickjs-wasi");
		const { PRELUDE_SOURCE } = await import(`${piCodemode()}/src/runtime/prelude-source.ts`);
		const vm = await QuickJS.create({ wasm: module, memoryLimit: 256 << 20 });
		let calls = 0;
		const bridge = vm.newFunction("bridge", (kind) => {
			if (kind.toString() === "call") calls++;
			return vm.undefined;
		});
		const task = byName("neg-fanout-40");
		const api = vm.callFunction(
			vm.evalCode(PRELUDE_SOURCE, "codemode-prelude.js"),
			vm.undefined,
			bridge,
			vm.newString(JSON.stringify(task.tools.map((t) => ({ name: t.name, jsName: t.name, description: t.description })))),
			vm.newString("[]"),
			vm.newString("{}"),
		);
		const fn = vm.evalCode(`(async (tools, console) => {${task.js}\n})`, "codemode.js");
		vm.callFunction(api.getProp("run"), api, fn).dispose();
		vm.executePendingJobs();
		const snapMs: number[] = [];
		let image: Uint8Array = new Uint8Array();
		for (let i = 0; i < 10; i++) {
			const t = performance.now();
			image = QuickJS.serializeSnapshot(vm.snapshot());
			snapMs.push(ms(t));
		}
		const restoreMs: number[] = [];
		for (let i = 0; i < 10; i++) {
			const t = performance.now();
			const r = await QuickJS.restore(QuickJS.deserializeSnapshot(image), { wasm: module });
			restoreMs.push(ms(t));
			r.dispose();
		}
		out.push({
			point: "neg-fanout-40@0",
			calls_open: calls,
			raw: image.length,
			zstd1: zstdCompressSync(image, { params: { 100: 1 } }).length,
			gzip6: gzipSync(image).length,
			snapshot_ms: median(snapMs),
			restore_ms: median(restoreMs),
		});
	}
	emit({ points: out });
} else if (cmd === "ksessions") {
	const [k, n] = [num(0, 1), num(1, 32)];
	const module = await WebAssembly.compile(bytes);
	const settle = async () => {
		for (let i = 0; i < 3; i++) {
			gc();
			await new Promise((r) => setTimeout(r, 20));
		}
	};
	// One session first, so the code the sessions run is compiled before RSS is read.
	(await kSessions(module, 1, 1, true, fixture.tools)).live.forEach((m) => m.s.dispose());
	await settle();
	const rss0 = process.memoryUsage().rss;
	const r = await kSessions(module, k, n, rest[2] === "setup", fixture.tools);
	await settle();
	const rss = process.memoryUsage().rss - rss0;
	const { live, ...out } = r;
	emit({ ...out, rss_per_session: rss / n });
	for (const m of live) m.s.dispose();
} else if (cmd === "fuel") {
	const module = await WebAssembly.compile(bytes);
	const what = rest[0];
	if (what === "programs") emit({ what, programs: fuelPrograms(module, loadPrograms(), num(1, 3)) });
	else if (what === "tasks") emit({ what, ...(await fuelTasks(module, tasks, num(1, 3))) });
	else emit({ what, mib: num(1, 16), probes: FUEL_PROBES.map((p) => fuelWorst(module, num(1, 16) << 20, p)) });
} else {
	console.error(`unknown command ${cmd}`);
	process.exit(2);
}
