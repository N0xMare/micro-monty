/**
 * The workerd side of stages.ts. Routes (GET, JSON): `/ping`, `/add?n`,
 * `/run?cell=cell|imports`, `/clear`, `/times?reps`; each answer carries the
 * live sessions' linear memory. run.py reads workerd's RSS between requests.
 */
import engineWasm from "./engine.wasm";
import { trackMemories } from "../../../hosts/js/bench/measure.ts";
import { CELLS, Stages } from "./stages.ts";

const linear = trackMemories();
const module = engineWasm as WebAssembly.Module;
let st: Stages | undefined;

export default {
	async fetch(req: Request): Promise<Response> {
		const url = new URL(req.url);
		const q = (k: string, d: number) => Number(url.searchParams.get(k) ?? d);
		try {
			const t = performance.now();
			st ??= new Stages(module);
			const out: Record<string, unknown> = {};
			switch (url.pathname) {
				case "/ping":
					break;
				case "/add":
					st.add(q("n", 1));
					break;
				case "/run":
					st.run(CELLS[url.searchParams.get("cell") ?? "cell"] as string);
					break;
				case "/clear":
					st.clear();
					break;
				case "/batch":
					st.batch(url.searchParams.get("m") ?? "", q("reps", 41));
					break;
				default:
					return new Response("not found", { status: 404 });
			}
			return Response.json({ ...out, live: st.live.length, linear: linear(), ms: performance.now() - t });
		} catch (e) {
			return Response.json({ error: e instanceof Error ? `${e.message}\n${e.stack}` : String(e) }, { status: 500 });
		}
	},
};
