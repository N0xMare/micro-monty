// A reset is a boundary (docs/spec.md): nothing a cell does reaches the cells
// after `session.reset()`. The probes of conformance/reset/probes.txt, with the
// walk (walk.py), on this host, as crates/micro-monty/tests/reset_boundary.rs
// runs them on the native facade and crates/mm-wasm-host/tests/reset_boundary.rs
// on the reference host: cell A, a reset, then cell B, which must report what
// it reports in a new session. The host's definitions and answers are those of
// conformance/reset/probes.rs.
//
// Needs the guests: `npm run guests` (MM_GUEST_DIR overrides hosts/js/guests).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
	anySignature,
	type HostModule,
	type HostSignature,
	type HostValue,
	PyObject,
	PyRecord,
	type Resume,
	Runtime,
	type Session,
	sessionOptions,
} from "../src/host.ts";

const here = dirname(fileURLToPath(import.meta.url));
const dir = process.env.MM_GUEST_DIR ?? join(here, "../guests");
const rt = await Runtime.create(readFileSync(join(dir, `${process.env.MM_GUEST ?? "default"}.wasm`)));
const options = sessionOptions({ limits: { memory: 64 << 20, recursionDepth: 1000, fuel: null } });
const reset = (name: string) => readFileSync(join(here, "../../../conformance/reset", name), "utf8");

const row = () => new PyRecord("Row", new Map<string, HostValue>([["a", 1]]));

function modules(): HostModule[] {
	const typed = (name: string, params: HostSignature["params"] = []): HostSignature => ({ name, params, returns: "object" });
	const read = typed("read", [
		{ name: "paths", default: ["/safe"] },
		{ name: "limit", default: 1, kind: "keyword-only" },
	]);
	// Record defaults: instances of the record class, which the functions keep through every reset.
	const record = (name: string) =>
		typed(name, [
			{ name: "row", default: row() },
			{ name: "rows", default: [row()], kind: "keyword-only" },
		]);
	// `dictAsKwargs`: the function a cell gets is a wrapper the host module's code names inside a function.
	const kwargs = (name: string): HostSignature => ({ ...read, name, dictAsKwargs: true });
	return [
		{
			name: "",
			functions: [typed("h"), typed("rec"), typed("obj"), typed("boom"), read, record("readr"), kwargs("readk"), anySignature("tool")],
			classes: [
				{
					name: "Entry",
					fields: [["path", "str"]],
					methods: [typed("go", [{ name: "n", default: 1 }]), record("gor"), kwargs("gok")],
				},
			],
			constants: [
				["CONF", ["a"]],
				["CONFD", new Map<string, HostValue>([["a", ["b"]]])],
				["CONFR", row()],
			],
			exceptions: [{ name: "ToolError", base: "RuntimeError" }],
		},
		{
			name: "fs",
			functions: [read, anySignature("raw")],
			classes: [{ name: "Node", fields: [["path", "str"]], methods: [] }],
			constants: [["ROOTS", ["/"]]],
			exceptions: [{ name: "FsError", base: "RuntimeError" }],
		},
	];
}

/** The host's answer; `undefined` leaves the cell stopped there (`tool('stop')`). */
function answer(name: string, args: HostValue[]): Resume | undefined {
	if (name === "tool" && args.length === 1 && args[0] === "stop") return undefined;
	if (name === "rec") return { kind: "value", value: row() };
	if (name === "obj") return { kind: "value", value: new PyObject("Entry", new Map<string, HostValue>([["path", "p"]])) };
	if (name === "boom") return { kind: "error", error: { kind: "RuntimeError", message: "bad", cls: "ToolError" } };
	return { kind: "value", value: null };
}

interface Cells {
	session: Session;
	out: string[];
}

function newSession(): Cells {
	const out: string[] = [];
	const session = rt.session(options);
	for (const m of modules()) session.define(m);
	session.setOutput((_, t) => {
		out.push(t);
	});
	return { session, out };
}

/** How a cell ended, and that with its host calls and output. */
function cell(c: Cells, source: string): [string, string] {
	c.out.length = 0;
	const calls: string[] = [];
	let p = c.session.start(source, "cell.py");
	let end: string;
	for (;;) {
		if (p.kind === "host-call") {
			calls.push(`${p.name}${JSON.stringify(p.args, (_, v: unknown) => (v instanceof Map ? [...v] : v))}`);
			const a = answer(p.name, p.args);
			if (a === undefined) {
				end = "stopped";
				break;
			}
			p = c.session.resume(a);
		} else if (p.kind === "complete") {
			end = p.completion?.repr ?? "None";
			break;
		} else if (p.kind === "error") {
			end = `${p.error.typeName}: ${p.error.message}`;
			break;
		} else {
			end = JSON.stringify(p);
			break;
		}
	}
	return [end, `${end}\ncalls ${calls.join(" ")}\n${c.out.join("")}`];
}

interface Probe {
	label: string;
	differs: boolean;
	aEnds?: string;
	a: string;
	b: string;
}

function probes(): Probe[] {
	const walk = reset("walk.py");
	const sweep = reset("sweep.py");
	const all: Probe[] = [];
	let inB = false;
	for (const line of reset("probes.txt").split("\n")) {
		if (line.startsWith("=== ") || line.startsWith("!== ")) {
			all.push({ label: line.slice(4), differs: line.startsWith("!"), a: "", b: "" });
			inB = false;
			continue;
		}
		const p = all.at(-1);
		if (p === undefined) continue;
		if (line === "---") inB = true;
		else if (line.startsWith(">>> ")) p.aEnds = line.slice(4);
		else {
			const text = line.startsWith("@walk ")
				? `MODE = '${line.slice(6)}'\n${walk}`
				: line.startsWith("@sweep ")
					? `MODE = '${line.slice(7)}'\n${sweep}`
					: `${line}\n`;
			if (inB) p.b += text;
			else p.a += text;
		}
	}
	return all;
}

test("a cell after a reset reports what it reports in a new session", () => {
	const all = probes();
	assert.ok(all.length > 60, `${all.length} probes`);
	const failures: string[] = [];
	for (const p of all) {
		const s = newSession();
		const [aEnd] = cell(s, p.a);
		if (p.aEnds !== undefined && p.aEnds !== aEnd) failures.push(`${p.label}: cell A ended in\n  ${aEnd}\nnot\n  ${p.aEnds}`);
		s.session.reset();
		const [, after] = cell(s, p.b);
		const [, expected] = cell(newSession(), p.b);
		if ((after === expected) === p.differs)
			failures.push(`${p.label}: after cell A and a reset, cell B reports\n${after}\nand in a new session\n${expected}`);
	}
	assert.equal(failures.join("\n\n"), "");
});
