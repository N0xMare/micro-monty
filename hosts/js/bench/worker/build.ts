// Bundles worker.ts for workerd into hosts/js/.bench/worker.js (pi's codemode from
// $PI_CODEMODE as `pi-codemode`, `engine.wasm` and `cloudflare:workers` left to
// workerd) after writing the 22 tasks to hosts/js/.bench/data.gen.json and the
// bench programs to programs.gen.json.
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { loadPrograms, loadTasks, piCodemode, ROOT } from "../data.ts";

const out = join(ROOT, "hosts/js/.bench");
mkdirSync(out, { recursive: true });
writeFileSync(join(out, "data.gen.json"), JSON.stringify(loadTasks()));
writeFileSync(join(out, "programs.gen.json"), JSON.stringify(loadPrograms()));
const bundles: [string, string[]][] = [
	["worker.js", []],
	["worker-warm.js", ["--define:WARMUP=true"]],
];
for (const [name, defines] of bundles) execFileSync(
	join(ROOT, "hosts/js/node_modules/.bin/esbuild"),
	[
		...defines,
		join(ROOT, "hosts/js/bench/worker/worker.ts"),
		"--bundle",
		"--format=esm",
		"--platform=neutral",
		"--target=es2024",
		"--main-fields=module,main",
		"--external:cloudflare:workers",
		"--external:./engine.wasm",
		`--alias:pi-codemode=${piCodemode()}/src/portable.ts`,
		`--outfile=${join(out, name)}`,
		"--log-level=warning",
	],
	{ stdio: "inherit" },
);
console.log(out);
