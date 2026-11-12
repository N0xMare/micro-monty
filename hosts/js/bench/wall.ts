/**
 * The V8 half of `cargo xtask wasm-wall`: runs bench programs on one mm-wasm
 * guest and prints one JSON line, `{"<program>": <ms>, ...}`.
 *
 *   node --no-liftoff bench/wall.ts GUEST.wasm REPS PROGRAM.py=EXPECTED [...]
 *
 * Under `--no-liftoff` V8 compiles the whole module with TurboFan up front, so
 * the times are steady state from the first run, independent of when tier-up
 * happens. Each program runs once to warm up, then REPS times, timed; it
 * reports the fastest. Every run has a fresh session. The run's output must equal
 * EXPECTED (its line in bench/expected.txt); otherwise the script exits 1.
 */
import { readFileSync } from "node:fs";
import { run } from "../src/driver.ts";
import { Runtime, sessionOptions } from "../src/host.ts";

const [guest = "", reps = "1", ...programs] = process.argv.slice(2);
const rt = Runtime.fromModule(new WebAssembly.Module(readFileSync(guest)));
const times: Record<string, number> = {};

for (const arg of programs) {
	const at = arg.indexOf("=");
	const [path, expected] = [arg.slice(0, at), arg.slice(at + 1)];
	const src = readFileSync(path, "utf8");
	const once = async (): Promise<number> => {
		const session = rt.session(sessionOptions({}));
		const out: string[] = [];
		session.setOutput((_, text) => {
			out.push(text);
		});
		const started = performance.now();
		const result = await run(session, src, { tools: {} });
		const ms = performance.now() - started;
		const text = out.join("").trim();
		if (result.kind !== "complete" || text !== expected) {
			process.stderr.write(`${path}: ${result.kind}, printed ${JSON.stringify(text.slice(-200))}\n`);
			process.exit(1);
		}
		return ms;
	};
	await once();
	let best = Number.POSITIVE_INFINITY;
	for (let i = 0; i < Number(reps); i++) best = Math.min(best, await once());
	times[path] = best;
}
process.stdout.write(`${JSON.stringify(times)}\n`);
