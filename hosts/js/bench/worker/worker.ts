/**
 * The measurements inside workerd (Cloudflare's runtime, run locally from the
 * `workerd` npm binary). build.ts bundles this file and writes one workerd
 * config per engine: the engine's wasm is the module `engine.wasm`, compiled by
 * workerd when the Worker loads, as Wrangler's `CompiledWasm` rule does on
 * Cloudflare (pi's cloudflare.ts imports quickjs-wasi the same way).
 *
 * Routes (GET, JSON): `/startup?reps`, `/mem/add?n`, `/mem/clear`,
 * `/hostcall?kind&n&reps`, `/workload?reps`, `/snapshot`, `/ksessions?k&n&setup`,
 * `/fuel/programs?reps`, `/fuel/tasks?reps`, `/fuel/worst?mib&probe` (mm only;
 * node.ts documents them), and the Durable Object demo `/hib/start?task&after`
 * and `/hib/resume` (class `Cells`).
 */
import { DurableObject } from "cloudflare:workers";
import { InlineCodemodeSandbox } from "pi-codemode";
import data from "../../.bench/data.gen.json" with { type: "json" };
import programs from "../../.bench/programs.gen.json" with { type: "json" };
import engineWasm from "./engine.wasm";
import { Mm, Pi, type PiSandboxClass, pyValues, Replay, type Task } from "../engines.ts";
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
} from "../measure.ts";

interface Env {
	ENGINE: string;
	CELLS: DurableObjectNamespace;
}

const tasks = data as Task[];
const byName = (n: string) => tasks.find((t) => t.task === n) as Task;
const module = engineWasm as WebAssembly.Module;
/** This isolate's identity, to show that a resumed cell ran in a new process. */
const ISOLATE = Math.random().toString(36).slice(2, 10);

/**
 * `worker-warm.js` (built with WARMUP defined) runs one session in the global scope,
 * so V8 compiles the functions a first cell needs during the Worker's startup (its
 * own 1 s limit) instead of during the first request.
 */
declare const WARMUP: boolean;
const isMm = WebAssembly.Module.exports(module).some((e) => e.name === "mm_abi_version");
if (typeof WARMUP !== "undefined" && WARMUP) {
	if (isMm) {
		const mm = new Mm(module);
		const m = mm.session();
		mm.setup(m, byName("neg-fanout-40").tools);
		m.s.start("import asyncio, json\nprint(1)\n");
		m.s.dispose();
	} else {
		const pi = new Pi(InlineCodemodeSandbox as unknown as PiSandboxClass, module);
		await pi.run(byName("neg-fanout-40").tools, "console.log(1);", () => ({ ok: true, value: null }));
	}
}

let engine: Engine | undefined;
const getEngine = (env: Env): Engine => {
	engine ??= env.ENGINE.startsWith("mm:")
		? { mm: new Mm(module) }
		: { pi: new Pi(InlineCodemodeSandbox as unknown as PiSandboxClass, module) };
	return engine;
};
const live: unknown[] = [];
const linear = trackMemories();

async function gzipSize(bytes: Uint8Array): Promise<number> {
	const s = new Blob([bytes as Uint8Array<ArrayBuffer>]).stream().pipeThrough(new CompressionStream("gzip"));
	return (await new Response(s).arrayBuffer()).byteLength;
}

export default {
	async fetch(req: Request, env: Env): Promise<Response> {
		const url = new URL(req.url);
		const q = (k: string, d: number) => Number(url.searchParams.get(k) ?? d);
		const json = (o: unknown) => Response.json({ engine: env.ENGINE, isolate: ISOLATE, ...(o as object) });
		try {
			switch (url.pathname) {
				case "/ping":
					return json({});
				case "/startup":
					return json(await startup(getEngine(env), byName("neg-fanout-40").tools, q("reps", 20)));
				case "/mem/add": {
					const e = getEngine(env);
					for (let i = 0, n = q("n", 1); i < n; i++) await liveSession(e, byName("neg-fanout-40").tools, live);
					return json({ live: live.length, linear: linear() });
				}
				case "/mem/clear":
					live.length = 0;
					return json({});
				case "/hostcall":
					return json(
						await hostcall(getEngine(env), url.searchParams.get("kind") as "await" | "fanout", q("n", 2000), q("reps", 5)),
					);
				case "/workload":
					return json(await workload(getEngine(env), tasks, q("reps", 5)));
				case "/snapshot": {
					const e = getEngine(env);
					if (!e.mm) return json({ supported: false });
					const points = [];
					for (const [name, after] of SNAP_POINTS) {
						const r = mmSnapshot(e.mm, byName(name), after);
						points.push({ ...r, image: undefined, raw: r.image.length, gzip: await gzipSize(r.image) });
					}
					return json({ points });
				}
				case "/ksessions": {
					const { live: held, ...r } = await kSessions(module, q("k", 1), q("n", 32), q("setup", 0) === 1, byName("neg-fanout-40").tools);
					for (const m of held) m.s.dispose();
					return json(r);
				}
				case "/fuel/programs":
					return json({ programs: fuelPrograms(module, programs, q("reps", 3)) });
				case "/fuel/tasks":
					return json(await fuelTasks(module, tasks, q("reps", 3)));
				case "/fuel/worst":
					return json(fuelWorst(module, q("mib", 16) << 20, FUEL_PROBES[q("probe", 0)] as (typeof FUEL_PROBES)[number]));
				case "/hib/start":
				case "/hib/resume":
					return env.CELLS.get(env.CELLS.idFromName("demo")).fetch(req);
				default:
					return new Response("not found", { status: 404 });
			}
		} catch (e) {
			return Response.json({ error: e instanceof Error ? `${e.message}\n${e.stack}` : String(e) }, { status: 500 });
		}
	},
};

/**
 * Hibernate and resume: a cell that waits on tools is parked, its image stored in
 * the object's SQLite storage with the calls it waits on, and the session dropped.
 * After an eviction (the runner restarts workerd), a new isolate restores the image
 * and finishes the cell with the tools' answers.
 */
export class Cells extends DurableObject<Env> {
	async fetch(req: Request): Promise<Response> {
		const url = new URL(req.url);
		const sql = this.ctx.storage.sql;
		sql.exec("CREATE TABLE IF NOT EXISTS parked (id TEXT PRIMARY KEY, image BLOB, meta TEXT)");
		const mm = new Mm(module);
		const t = () => performance.now();
		if (url.pathname === "/hib/start") {
			const task = byName(url.searchParams.get("task") ?? "neg-fanout-40");
			const after = Number(url.searchParams.get("after") ?? 0);
			let t0 = t();
			const m = mm.session();
			mm.setup(m, task.tools);
			const replay = new Replay(task, pyValues(task));
			const stop = mm.runUntil(m, task.py, replay, after);
			if (!stop) throw new Error("the cell did not wait");
			const run_ms = t() - t0;
			t0 = t();
			const image = m.s.snapshot();
			const snapshot_ms = t() - t0;
			const answered = replay.calls.slice(0, replay.calls.length - stop.open.length);
			t0 = t();
			sql.exec(
				"INSERT OR REPLACE INTO parked VALUES ('cell', ?, ?)",
				image,
				JSON.stringify({ task: task.task, open: stop.open, answered, output: m.out.join("") }),
			);
			const store_ms = t() - t0;
			const linear = m.s.memoryBytes();
			m.s.dispose();
			return Response.json({
				isolate: ISOLATE,
				task: task.task,
				open_calls: stop.open.length,
				image_bytes: image.length,
				linear_bytes: linear,
				run_ms,
				snapshot_ms,
				store_ms,
			});
		}
		if (url.pathname === "/hib/resume") {
			let t0 = t();
			const row = sql.exec<{ image: ArrayBuffer; meta: string }>("SELECT image, meta FROM parked WHERE id = 'cell'").one();
			const meta = JSON.parse(row.meta) as {
				task: string;
				open: [number, string, unknown][];
				answered: [string, unknown][];
				output: string;
			};
			const load_ms = t() - t0;
			t0 = t();
			const s = mm.rt.restore(new Uint8Array(row.image), mm.options());
			const restore_ms = t() - t0;
			const task = byName(meta.task);
			const r = { s, out: [meta.output], params: new Map(task.tools.map((x) => [x.name, x.params])) };
			s.setOutput((_, x) => {
				r.out.push(x);
			});
			const replay = new Replay(task, pyValues(task));
			for (const [n, a] of meta.answered) replay.call(n, a);
			const pending = s.pending();
			t0 = t();
			const res = mm.finish(r, meta.open, replay);
			const finish_ms = t() - t0;
			sql.exec("DELETE FROM parked WHERE id = 'cell'");
			return Response.json({
				isolate: ISOLATE,
				task: task.task,
				pending: pending?.kind,
				image_bytes: row.image.byteLength,
				load_ms,
				restore_ms,
				finish_ms,
				error: res.error ?? null,
				output: res.output,
				calls: replay.calls,
			});
		}
		return new Response("not found", { status: 404 });
	}
}
