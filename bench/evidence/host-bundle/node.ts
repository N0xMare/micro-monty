/**
 * node --expose-gc bench/evidence/host-bundle/node.ts GUEST.wasm N
 * One JSON line: RSS, V8 heap and linear memory per session idle, after a cell
 * and after importing json, re and collections; then first-import times.
 */
import { readFileSync } from "node:fs";
import { trackMemories } from "../../../hosts/js/bench/measure.ts";
import { CELLS, Stages } from "./stages.ts";

const linear = trackMemories();
const [path = "", nArg = "40"] = process.argv.slice(2);
const n = Number(nArg);
const gc = (globalThis as { gc?: () => void }).gc ?? (() => {});
const settle = async () => {
	for (let i = 0; i < 3; i++) {
		gc();
		await new Promise((r) => setTimeout(r, 20));
	}
};
const sample = () => {
	const u = process.memoryUsage();
	return { rss: u.rss, heap: u.heapUsed, linear: linear() };
};
let t = performance.now();
const module = await WebAssembly.compile(readFileSync(path));
const compile_ms = performance.now() - t;
t = performance.now();
const st = new Stages(module);
const runtime_ms = performance.now() - t;
st.add(1);
st.run(CELLS.imports);
st.clear();
await settle();
const base = sample();
const per = (s: ReturnType<typeof sample>) =>
	Object.fromEntries(Object.entries(s).map(([k, v]) => [k, Math.round((v - base[k as keyof typeof base]) / n / 1024)]));
st.add(n);
await settle();
const idle = per(sample());
st.run(CELLS.cell);
await settle();
const cell = per(sample());
st.run(CELLS.imports);
await settle();
const imports = per(sample());
st.clear();
await settle();
const times = st.times(41);
process.stdout.write(
	`${JSON.stringify({ guest: path, n, compile_ms, runtime_ms, kib_per_session: { idle, cell, imports }, us: times })}\n`,
);
