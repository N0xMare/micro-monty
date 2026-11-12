// Loads the 22 codemode tasks of bench/evidence/engines: the recorded fixture, the
// set-`s` Python solution (conformance/codemode/solutions/s) and the pi-style JS
// solution (bench/evidence/engines/js). Node only; the Worker gets the same data
// as one JSON module (worker/build.ts).
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Task } from "./engines.ts";

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const ENGINES = join(ROOT, "bench/evidence/engines");

export function loadTasks(): Task[] {
	return readdirSync(join(ENGINES, "fixtures"))
		.filter((f) => f.endsWith(".json"))
		.sort()
		.map((f) => {
			const name = f.slice(0, -5);
			const text = readFileSync(join(ENGINES, "fixtures", f), "utf8");
			const v = JSON.parse(text);
			return {
				task: v.task,
				tools: v.tools,
				calls: v.calls,
				text,
				py: readFileSync(join(ROOT, "conformance/codemode/solutions/s", `${name}.py`), "utf8"),
				js: readFileSync(join(ENGINES, "js", `${name}.js`), "utf8"),
			};
		});
}

export const guestPath = (variant: string) =>
	join(process.env.MM_GUEST_DIR ?? join(ROOT, "hosts/js/guests"), `${variant}.wasm`);
export const quickjsPath = () => join(ROOT, "hosts/js/node_modules/quickjs-wasi/quickjs.wasm");
/** pi's codemode package source (`packages/codemode` at the harness's pin), `$PI_CODEMODE`. */
export const piCodemode = () => process.env.PI_CODEMODE ?? "/tmp/eng/pi/packages/codemode";

/** A bench/ program (docs/spec.md §12) and the checksum it prints (bench/expected.txt, without the name). */
export interface Program {
	name: string;
	py: string;
	expected: string;
}

/** The bench/*.py programs, `startup` (empty) left out. */
export function loadPrograms(): Program[] {
	const expected = new Map(
		readFileSync(join(ROOT, "bench/expected.txt"), "utf8")
			.split("\n")
			.filter((l) => l && !l.startsWith("#"))
			.map((l) => [l.slice(0, l.indexOf(":")), l.slice(l.indexOf(":") + 2)] as const),
	);
	return readdirSync(join(ROOT, "bench"))
		.filter((f) => f.endsWith(".py") && f !== "startup.py" && f !== "opcost.py" && f !== "callgrind.py")
		.sort()
		.map((f) => {
			const name = f === "json_rows.py" ? "json" : f.slice(0, -3);
			return { name, py: readFileSync(join(ROOT, "bench", f), "utf8"), expected: expected.get(name) ?? "" };
		});
}
