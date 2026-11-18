/**
 * The V8 half of `cargo xtask v8-ir`: runs one bench program twice on one
 * mm-wasm guest under callgrind, which xtask starts with
 * `--dump-after=uv_getrusage`.
 *
 *   node --no-liftoff --single-threaded bench/count.ts GUEST.wasm PROGRAM.py=EXPECTED
 *
 * xtask runs it from a copy of this directory under `/tmp` (see
 * `xtask/src/v8_ir.rs`).
 *
 * `process.resourceUsage()` calls libuv's `uv_getrusage`, so each call ends
 * a callgrind dump: the first after the module is compiled, then one after
 * each run. The second run's dump is what xtask counts: it repeats, though
 * it starts with a deopt of the session's host calls and 0 to 2 recompiles; the
 * first run also holds V8's lazy TurboFan compiles. Every run has a fresh
 * session, disposed after it. A run that does not complete, or prints
 * other than EXPECTED, exits 1 and says which. Last, the process's mappings
 * go to `maps.txt`.
 */
import { readFileSync, writeFileSync } from "node:fs";
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
// After the last dump, so not counted: the process's mappings, by which xtask
// leaves out code in a file (libstdc++'s and libc's stubs) and keeps V8's.
writeFileSync("maps.txt", readFileSync("/proc/self/maps"));
