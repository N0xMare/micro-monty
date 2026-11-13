/**
 * The V8 half of `cargo xtask v8-ir`: runs one bench program twice on one
 * mm-wasm guest under callgrind, which xtask starts with
 * `--dump-after=uv_getrusage`.
 *
 *   node --no-liftoff bench/count.ts GUEST.wasm PROGRAM.py=EXPECTED
 *
 * `process.resourceUsage()` calls libuv's `uv_getrusage`, so each call ends
 * a callgrind dump: the first after the module is compiled, then one after
 * each run. The second run's dump is the steady state xtask counts; the
 * first run also holds V8's lazy TurboFan compiles. Every run has a fresh
 * session, disposed after it. A run that does not complete, or prints
 * other than EXPECTED, exits 1 and says which.
 */
import { readFileSync } from "node:fs";
import { run } from "../src/driver.ts";
import { Runtime, sessionOptions } from "../src/host.ts";

const [guest = "", arg = ""] = process.argv.slice(2);
const at = arg.indexOf("=");
const [path, expected] = [arg.slice(0, at), arg.slice(at + 1)];
const src = readFileSync(path, "utf8");
const rt = Runtime.fromModule(new WebAssembly.Module(readFileSync(guest)));
process.resourceUsage();
for (let i = 1; i <= 2; i++) {
	const session = rt.session(sessionOptions({}));
	const out: string[] = [];
	session.setOutput((_, text) => {
		out.push(text);
	});
	const result = await run(session, src, { tools: {} });
	const text = out.join("").trim();
	session.dispose();
	if (result.kind !== "complete") {
		process.stderr.write(`count: ${path} run ${i} ended ${result.kind}\n`);
		process.exit(1);
	}
	if (text !== expected) {
		process.stderr.write(`count: ${path} run ${i} printed ${JSON.stringify(text.slice(-200))}\n`);
		process.exit(1);
	}
	process.resourceUsage();
}
