// The JS host against the scenarios of the reference host's tests
// (crates/mm-wasm-host/tests/parity.rs and upgrade.rs). The reference host
// compares the guest with the native facade; here the same scenarios run on the
// same shipped guest and check the facade's documented results directly.
//
// Needs the guests: `npm run guests` (MM_GUEST_DIR overrides hosts/js/guests).
// MM_GUEST names the guest the scenarios run on: `default`, or `bundle` for the
// bundle guest (its bytecode served from its custom sections).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { Worker } from "node:worker_threads";
import { run } from "../src/driver.ts";
import {
	Float,
	type HostModule,
	type HostValue,
	MmError,
	option,
	type Progress,
	putOptions,
	Runtime,
	type Session,
	type SessionOptions,
	sessionOptions,
	Tuple,
	Unconverted,
	untypedModule,
	Writer,
} from "../src/host.ts";

const dir = process.env.MM_GUEST_DIR ?? join(dirname(fileURLToPath(import.meta.url)), "../guests");
const guest = (name: string) => readFileSync(join(dir, `${name}.wasm`));
const main = process.env.MM_GUEST ?? "default";
const rt = await Runtime.create(guest(main));
const LIMITS = { memory: 64 << 20, recursionDepth: 200, fuel: null };
const options = sessionOptions({ limits: LIMITS });

function module(): HostModule {
	return {
		name: "tools",
		functions: [
			{
				name: "search",
				params: [
					{ name: "query", annotation: "str" },
					{ name: "limit", annotation: "int" },
				],
				returns: "list",
				doc: "Search.",
				dictAsKwargs: true,
			},
			{ name: "fetch", params: [{ name: "args", kind: "var-positional" }, { name: "kwargs", kind: "var-keyword" }] },
		],
		constants: [["VERSION", 3]],
		exceptions: [{ name: "ToolError", base: "RuntimeError", doc: "A tool failed." }],
	};
}

const zones = (key: string) => {
	try {
		return readFileSync(`/usr/share/zoneinfo/${key}`);
	} catch {
		return undefined;
	}
};

test("host errors, kwargs, call log, hints, text cut, zones, fuel_wanted, stubs", () => {
	const CELL = `import tools
from zoneinfo import ZoneInfo
from datetime import datetime
r = tools.search({'query': 'q', 'limit': 2})
try:
    tools.fetch('x')
except tools.ToolError as e:
    print('caught', type(e).__name__, e.tool, e.data, isinstance(e, RuntimeError))
paris = ZoneInfo('Europe/Paris')
print(datetime(2026, 7, 1, 12, tzinfo=paris).utcoffset())
x = 7 ** 100000
list(range(300))
`;
	const s = rt.session(sessionOptions({ limits: LIMITS, textLimit: 40 }));
	let out = "";
	s.setOutput((_, t) => {
		out += t;
	});
	s.setZoneSource(zones);
	s.setHint("network", "Use the http tool.");
	s.setTextCut("head-tail");
	const refs = s.define(module());
	assert.deepEqual(
		refs.map((r) => r.name),
		["tools.search", "tools.fetch"],
	);
	s.setFuel(5_000);
	const wanted: number[] = [];
	const calls: Progress[] = [];
	let p = s.start(CELL, "cell.py");
	for (;;) {
		if (p.kind === "host-call") {
			calls.push(p);
			p =
				p.name === "tools.search"
					? s.resume({ kind: "value", value: [1] })
					: s.resume({
							kind: "error",
							error: { kind: "Exception", message: "down", data: { code: 503 }, cls: "tools.ToolError" },
						});
		} else if (p.kind === "parked" && p.reason === "fuel") {
			const w = s.fuelWanted();
			wanted.push(w);
			s.setFuel(Math.max(w, 5_000));
			p = s.resume({ kind: "continue" });
		} else break;
	}
	// dict_as_kwargs binds the dict to the typed parameters.
	const search = calls[0];
	assert.ok(search?.kind === "host-call");
	assert.deepEqual(search.args, ["q", 2]);
	assert.ok(out.includes("caught ToolError tools.fetch {'code': 503} True"), out);
	assert.ok(out.includes("2:00:00\n"), out);
	assert.ok(
		wanted.some((w) => w > 5_000),
		`the big int parked before starting: ${wanted}`,
	);
	assert.ok(p.kind === "complete" && p.completion?.reprTruncated, JSON.stringify(p));
	assert.ok(p.kind === "complete" && p.completion?.repr.includes("bytes omitted"));
	assert.deepEqual(
		s.callLog().map((c) => [c.name, c.status]),
		[
			["tools.search", "returned"],
			["tools.fetch", "raised"],
		],
	);
	assert.ok(s.stubs().some(([m, t]) => m === "tools" && t.includes("def search")));
	assert.ok(s.environment().includes("tools"));

	// The hint reaches the uncaught exception, cut at the text limit: the traceback keeps its end.
	const hinted = s.start("import requests", "h.py");
	assert.ok(hinted.kind === "error", JSON.stringify(hinted));
	assert.equal(hinted.error.message, "No module named 'requests'");
	assert.ok(hinted.error.truncated && hinted.error.traceback.endsWith("ool.\n"), hinted.error.traceback);

	// The image restores with its definitions, call log and globals.
	const r = rt.restore(s.snapshot(), options);
	assert.deepEqual(r.callLog(), s.callLog());
	let tail = "";
	r.setOutput((_, t) => {
		tail += t;
	});
	r.start("print(len(r), tools.VERSION, paris.key)\nr");
	assert.equal(tail, "1 3 Europe/Paris\n");
});

test("host modules, module dunders, int bits text and GC stress, before and after a restore", () => {
	const DUNDERS = "import tools\ndef f(): pass\nclass C: pass\nprint(__name__, __file__, f.__module__, repr(C).split()[1])\n";
	const BIG = "[2 ** 20000, {'k': -(3 ** 20000)}]";
	const GARBAGE =
		"a = []; a.append(a)\nfor i in range(200):\n    b = [i, (i, str(i)), {'x': [i]}]\nprint(len(a[0][0]), b[2]['x'][0])\n";
	const s = rt.session(options);
	let out = "";
	s.setOutput((_, t) => {
		out += t;
	});
	s.define(module());
	s.define(untypedModule("", ["lookup"]));
	s.setModuleDunders("job", "/work/job.py");
	s.setIntBitsText(true);
	s.setGcStress(true);
	const results = [DUNDERS, BIG, GARBAGE].map((src) => s.start(src, "c.py"));
	assert.ok(out.startsWith("job /work/job.py job 'job.C'>\n"), out);
	assert.ok(out.endsWith("1 199\n"), out);
	const big = results[1];
	assert.ok(big?.kind === "complete" && big.completion?.repr.includes("<int with 20001 bits>"), JSON.stringify(big));
	const modules = s.hostModules();
	assert.deepEqual(
		modules.map((m) => m.name),
		["", "tools"],
	);
	assert.deepEqual(modules[1]?.exceptions, [{ name: "ToolError", base: "RuntimeError", doc: "A tool failed." }]);
	assert.equal(modules[1]?.functions[0]?.dictAsKwargs, true);

	// Definitions survive the image; the host-side settings are the restoring host's:
	// given in the options record, or set afterwards.
	const image = s.snapshot();
	const r = rt.restore(image, options);
	assert.deepEqual(r.hostModules(), modules);
	const before = r.start(BIG, "c.py");
	assert.ok(before.kind === "error" && before.error.typeName === "ValueError", JSON.stringify(before));
	assert.deepEqual(rt.restore(image, sessionOptions({ limits: LIMITS, intBitsText: true })).start(BIG, "c.py"), big);
	r.setIntBitsText(true);
	assert.deepEqual(r.start(BIG, "c.py"), big);
});

test("the options record: each option reaches a new session; a restore takes the host-side ones", () => {
	// Only what differs from the defaults is written: tag, u32 length, payload.
	const bytes = (o: SessionOptions) => {
		const w = new Writer();
		putOptions(w, o);
		return [...w.bytes()];
	};
	assert.deepEqual(bytes(sessionOptions()), [0, 0, 0, 0]);
	assert.deepEqual(bytes(sessionOptions({ gcStress: true })), [6, 0, 0, 0, option.GC_STRESS, 1, 0, 0, 0, 1]);
	assert.deepEqual(
		bytes(sessionOptions({ limits: { recursionDepth: 7 }, textCut: "head-tail" })),
		[15, 0, 0, 0, option.RECURSION_DEPTH, 4, 0, 0, 0, 7, 0, 0, 0, option.TEXT_CUT, 1, 0, 0, 0, 1],
	);
	assert.equal(rt.session(sessionOptions({ limits: LIMITS, gcStress: true })).start("[1] * 3", "c.py").kind, "complete");

	// Every other option, against what the defaults do.
	const all = sessionOptions({
		limits: { memory: 48 << 20, recursionDepth: 100, fuel: 100_000 },
		textLimit: 64,
		hostArgs: { maxBytes: 1000, maxDepth: 3, lenient: true },
		wallClock: false,
		entropy: false,
		zoneSource: false,
		textCut: "head-tail",
		intBitsText: true,
	});
	const s = rt.session(all);
	const plain = rt.session(sessionOptions());
	for (const t of [s, plain]) {
		t.define(untypedModule("", ["tool"]));
		t.setZoneSource(zones);
	}
	assert.ok(s.environment().includes("Limits: memory 48 MiB, recursion depth 100, 100000 fuel units per cell."));
	assert.ok(plain.environment().includes("Limits: memory 256 MiB, recursion depth 1000, no instruction budget."), plain.environment());
	const run = (t: Session, src: string) => {
		const p = t.start(src, "c.py");
		if (p.kind === "host-call") t.cancel();
		return p;
	};
	const raised = (p: Progress) => (p.kind === "error" ? p.error.typeName : p.kind);
	const RECURSE = "def f(n):\n    return n if n == 150 else f(n + 1)\nf(0)";
	const CLOCKS = ["import time\ntime.time()", "import os\nos.urandom(4)", "from zoneinfo import ZoneInfo\nZoneInfo('Europe/Paris')"];
	const NOT_FOUND = "zoneinfo._common.ZoneInfoNotFoundError";
	assert.deepEqual(
		[RECURSE, ...CLOCKS].map((src) => raised(run(s, src))),
		["RecursionError", "RuntimeError", "RuntimeError", NOT_FOUND],
	);
	assert.deepEqual(
		[RECURSE, ...CLOCKS].map((src) => raised(run(plain, src))),
		["complete", "complete", "complete", "complete"],
	);
	const texts = (t: Session) =>
		["list(range(300))", "2 ** 20000"].map((src) => {
			const p = t.start(src, "c.py");
			assert.ok(p.kind === "complete" && p.completion !== null, JSON.stringify(p));
			return p.completion.repr;
		});
	const [cut, bits] = texts(s);
	assert.ok(cut?.startsWith("[0, 1, 2") && cut.includes("bytes omitted") && cut.endsWith("299]"), cut);
	assert.equal(bits, "<int with 20001 bits>");
	const args = (t: Session, src: string) => {
		const p = run(t, src);
		assert.ok(p.kind === "host-call", JSON.stringify(p));
		return p.args;
	};
	const [set] = args(s, "tool({1, 2})");
	assert.ok(set instanceof Unconverted && set.kind === "opaque" && set.typeName === "set", JSON.stringify(set));
	const [long] = args(s, "tool('x' * 5000)");
	assert.ok(long instanceof Unconverted && long.kind === "too-large", JSON.stringify(long));
	let deep: unknown = args(s, "tool([[[[[1]]]]])")[0];
	while (Array.isArray(deep)) deep = deep[0];
	assert.ok(deep instanceof Unconverted && deep.kind === "too-deep", JSON.stringify(deep));
	assert.deepEqual(args(plain, "tool('x' * 5000)"), ["x".repeat(5000)]);
	assert.deepEqual(args(plain, "tool([[[[[1]]]]])"), [[[[[[1]]]]]]);
	const quiet = rt.session(sessionOptions({ limits: LIMITS, lastExprValue: false }));
	assert.deepEqual(quiet.start("1 + 1", "c.py"), { kind: "complete", completion: null });

	// A restore takes the limits and the host-side settings from its options, and the
	// text limit, lastExprValue and hostArgs from the image.
	const image = s.snapshot();
	const back = rt.restore(image, sessionOptions({ textLimit: 4096, hostArgs: { maxBytes: 1 << 30 } }));
	assert.ok(back.environment().includes("Limits: memory 48 MiB, recursion depth 100, no instruction budget."), back.environment());
	assert.deepEqual(
		CLOCKS.map((src) => raised(run(back, src))),
		["complete", "complete", NOT_FOUND],
	);
	const headCut = back.start("list(range(300))", "c.py");
	assert.ok(
		headCut.kind === "complete" && headCut.completion?.reprTruncated === true && !headCut.completion.repr.includes("omitted"),
		JSON.stringify(headCut),
	);
	assert.equal(raised(back.start("2 ** 20000", "c.py")), "ValueError");
	const [stillCapped] = args(back, "tool('x' * 5000)");
	assert.ok(stillCapped instanceof Unconverted && stillCapped.kind === "too-large", JSON.stringify(stillCapped));
	const same = rt.restore(image, all);
	same.setZoneSource(zones);
	assert.deepEqual(
		CLOCKS.map((src) => raised(run(same, src))),
		["RuntimeError", "RuntimeError", NOT_FOUND],
	);
	assert.deepEqual(texts(same), [cut, bits]);
});

test("fuel total, phase, typed define, resume and snapshot errors, truncation", () => {
	const LOOP = "t = 0\nfor i in range(5000):\n    t += i\nt";
	const s = rt.session(options);
	// u64::MAX, "none", crosses as i64 -1.
	assert.deepEqual([s.fuelTotal(), s.fuel()], [null, null]);
	s.setFuelTotal(3_000);
	assert.ok(s.environment().includes("3000 fuel units left for the session"));
	assert.equal(s.phase(), "idle");
	assert.deepEqual(s.start(LOOP, "l.py"), { kind: "parked", reason: "fuel" });
	assert.equal(s.phase(), "parked");
	assert.equal(s.fuelTotal(), 0);
	assert.throws(
		() => s.resume({ kind: "future" }),
		(e: unknown) =>
			e instanceof MmError && e.kind === "resume" && JSON.stringify(e.detail) === '{"kind":"unexpected","phase":"parked"}',
	);
	assert.equal(rt.restore(s.snapshot(), options).fuelTotal(), 0);
	assert.equal(s.cancel(), true);

	for (const m of [untypedModule("json", ["f"]), untypedModule("a-b", ["f"])]) {
		assert.throws(
			() => s.define(m),
			(e: unknown) => e instanceof MmError && e.kind === "define" && (e.detail as { kind: string }).kind === "invalid",
		);
	}

	const image = s.snapshot();
	image.set([3, 0], 8);
	assert.throws(
		() => rt.restore(image, options),
		(e: unknown) =>
			e instanceof MmError &&
			e.kind === "snapshot" &&
			(e.detail as { kind: string; format: number }).kind === "incompatible" &&
			(e.detail as { format: number }).format === 3,
	);

	const CUT = "fs = 'f'\nfor _ in range(22):\n    fs = frozenset([fs, (fs, 1)])\n{0: 1}[fs]\n";
	const t = rt.session(sessionOptions({ limits: LIMITS, textLimit: 4096 }));
	const a = t.start(CUT, "c.py");
	assert.ok(a.kind === "error" && a.error.truncated && new TextEncoder().encode(a.error.message).length === 4096);
	const b = t.start("1 / 0", "c.py");
	assert.ok(b.kind === "error" && !b.error.truncated && b.error.typeName === "ZeroDivisionError");

	assert.throws(
		() => t.start("def (", "bad.py"),
		(e: unknown) => e instanceof MmError && e.kind === "compile",
	);
});

test("pending re-reports every stop, also on restored sessions", () => {
	const CELL =
		"import asyncio\nx = fetch(1, 'two', [2.5, None, (b'\\x00',)], k={'a': 10 ** 30})\nasync def main():\n    return await asyncio.gather(slow(1), slow(2))\nasyncio.run(main())\n";
	const s = rt.session(options);
	s.define(untypedModule("", ["fetch", "slow"]));
	assert.equal(s.pending(), null);
	let p = s.start(CELL, "p.py");
	let stops = 0;
	while (p.kind !== "complete") {
		assert.deepEqual(s.pending(), p);
		assert.deepEqual(rt.restore(s.snapshot(), options).pending(), p);
		stops++;
		if (p.kind === "host-call" && p.name === "fetch") {
			assert.deepEqual(p.args, [1, "two", [new Float(2.5).value, null, new Tuple([new Uint8Array([0])])]]);
			assert.deepEqual(p.kwargs, new Map([["k", new Map([["a", 10n ** 30n]])]]));
			p = s.resume({ kind: "value", value: null });
		} else if (p.kind === "host-call") p = s.resume({ kind: "future" });
		else if (p.kind === "waiting") {
			const c = p.calls[0] as number;
			p = s.resume({ kind: "batch", items: [[c, { ok: true, value: c }]] });
		} else assert.fail(JSON.stringify(p));
	}
	assert.equal(s.pending(), null);
	assert.equal(stops, 5);
});

test("values cross both ways exactly", () => {
	const s = rt.session(options);
	s.define(untypedModule("", ["echo"]));
	const sent: HostValue[] = [
		1,
		2n ** 70n,
		new Float(2),
		2.5,
		-0,
		"é",
		new Uint8Array([1, 2]),
		new Tuple([null, true]),
		new Map<string, HostValue>([
			["2", 1],
			["1", 2],
		]),
	];
	let p = s.start("x = echo()\nprint(repr(x))\nx", "v.py");
	let out = "";
	s.setOutput((_, t) => {
		out += t;
	});
	assert.equal(p.kind, "host-call");
	p = s.resume({ kind: "value", value: sent });
	assert.equal(out, "[1, 1180591620717411303424, 2.0, 2.5, -0.0, 'é', b'\\x01\\x02', (None, True), {'2': 1, '1': 2}]\n");
	assert.ok(p.kind === "complete");
	// Integral floats come back as `Float`, -0.0 too.
	assert.deepEqual(p.completion?.value, [...sent.slice(0, 4), new Float(-0), ...sent.slice(5)]);
});

test("a park request between calls parks the next call instead of trapping", () => {
	const s = rt.session();
	assert.equal(s.start("1", "<a>").kind, "complete");
	s.requestPark();
	assert.equal(s.fuelWanted(), 0);
	s.define({ name: "t", functions: [{ name: "f", params: [] }] });
	let p = s.start("import t\nt.f()\nfor i in range(10**6): pass\n2", "<b>");
	if (p.kind === "parked") p = s.resume({ kind: "continue" });
	assert.equal(p.kind, "host-call");
	s.requestPark();
	p = s.resume({ kind: "value", value: null });
	assert.deepEqual(p, { kind: "parked", reason: "requested" });
	assert.equal(s.isDead, false);
});

test("a park request from inside a running cell (an output callback) parks it", () => {
	const s = rt.session();
	s.setOutput(() => s.requestPark());
	const p = s.start("print('go')\nn = 0\nwhile True:\n    n += 1\n", "loop.py");
	assert.deepEqual(p, { kind: "parked", reason: "requested" });
	assert.deepEqual(s.pending(), p);
	assert.deepEqual(rt.restore(s.snapshot()).pending(), p);
	s.setOutput(undefined);
	assert.ok(s.cancel());
	const q = s.start("6 * 7");
	assert.ok(q.kind === "complete" && q.completion?.repr === "42");
});

test("another thread parks a busy loop through the shared park signal", async () => {
	const signal = new Int32Array(new SharedArrayBuffer(4));
	const prt = await Runtime.create(guest(main), { parkSignal: signal });
	const s = prt.session();
	const worker = new Worker(
		"const { workerData } = require('node:worker_threads'); setTimeout(() => Atomics.store(workerData, 0, 1), 50);",
		{ eval: true, workerData: signal },
	);
	const t = performance.now();
	const p = s.start("n = 0\nwhile True:\n    n += 1\n", "loop.py");
	const ms = performance.now() - t;
	await worker.terminate();
	assert.deepEqual(p, { kind: "parked", reason: "requested" });
	assert.ok(ms >= 40 && ms < 1000, `${ms} ms`);
	assert.equal(Atomics.load(signal, 0), 0, "the guest took the request");
});

test("a deadline parks a running loop, and is one-shot", () => {
	const s = rt.session();
	s.setDeadline(30);
	const t = performance.now();
	const p = s.start("n = 0\nwhile n < 10**9:\n    n += 1\n", "loop.py");
	assert.deepEqual(p, { kind: "parked", reason: "deadline" });
	assert.ok(performance.now() - t >= 25);
	assert.deepEqual(s.pending(), { kind: "parked", reason: "deadline" });
	s.cancel();
	assert.equal(s.start("1").kind, "complete");
});

test("builds: restores pick the module that wrote the image; unknown builds are refused", async () => {
	const up = await Runtime.create(guest(main));
	const old = up.currentBuild();
	const s = up.session();
	s.start("x = 41");
	const image = s.snapshot();
	const fresh = await up.load(guest("core35"));
	assert.notDeepEqual(fresh, old);
	assert.equal(up.builds().length, 2);
	const r = up.restore(image);
	assert.deepEqual(r.buildId(), old);
	const q = r.start("x + 1");
	assert.ok(q.kind === "complete" && q.completion?.repr === "42");
	assert.deepEqual(up.session().buildId(), fresh);
	assert.equal(up.unload(fresh), false, "the current build stays");
	assert.equal(up.unload(old), true);
	assert.throws(
		() => up.restore(image),
		(e: unknown) => e instanceof MmError && e.kind === "unknown-build",
	);
	assert.throws(
		() => up.restore(new Uint8Array([1, 2, 3])),
		(e: unknown) => e instanceof MmError && e.kind === "snapshot",
	);
});

test("the default and bundle guests share a build: images cross between them", async () => {
	const a = await Runtime.create(guest("default"));
	const b = await Runtime.create(guest("bundle"));
	assert.deepEqual(a.currentBuild(), b.currentBuild());
	for (const [from, to] of [
		[a, b],
		[b, a],
	] as const) {
		const s = from.session();
		assert.equal(s.start("import json, re, collections\nx = json.dumps(collections.Counter('abca'))").kind, "complete");
		const r = to.restore(s.snapshot());
		const q = r.start("x + re.sub('a', 'b', 'aa')");
		assert.ok(q.kind === "complete" && q.completion?.repr === `'{"a": 2, "b": 1, "c": 1}bb'`, JSON.stringify(q));
	}
});

test("a bundle guest without its sections is refused at load", async () => {
	// The bundle guest's bytes with its custom sections (id 0) dropped.
	const b = guest("bundle");
	const parts: Uint8Array[] = [b.subarray(0, 8)];
	for (let i = 8; i < b.length; ) {
		const start = i++;
		let size = 0;
		for (let shift = 0; ; shift += 7) {
			const x = b[i++] as number;
			size += (x & 0x7f) * 2 ** shift;
			if (x < 0x80) break;
		}
		if (b[start] !== 0) parts.push(b.subarray(start, i + size));
		i += size;
	}
	const stripped = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
	parts.reduce((at, p) => (stripped.set(p, at), at + p.length), 0);
	await assert.rejects(Runtime.create(stripped), (e: unknown) => e instanceof MmError && e.kind === "abi");
});

test("the driver: awaited calls, a fan-out answered out of order, sync tools, errors", async () => {
	const s = rt.session();
	s.define({
		name: "",
		functions: [
			{ name: "slow", params: [{ name: "i", kind: "keyword-only" }], dictAsKwargs: true },
			{ name: "fail", params: [] },
			{ name: "now", params: [] },
		],
	});
	let out = "";
	s.setOutput((_, t) => {
		out += t;
	});
	const order: number[] = [];
	const r = await run(
		s,
		`import asyncio
a = await slow(i=0)
xs = await asyncio.gather(*[slow({'i': i}) for i in range(1, 6)])
try:
    await fail()
except ValueError as e:
    print('caught', e)
print(a, xs, now())
`,
		{
			sync: new Set(["now"]),
			tools: {
				slow: async (c) => {
					const i = c.args[0] as number;
					await new Promise((res) => setTimeout(res, 30 - i * 5));
					order.push(i);
					return i * 10;
				},
				fail: async () => {
					throw { kind: "ValueError", message: "nope" };
				},
				now: () => 7,
			},
		},
	);
	assert.equal(r.kind, "complete");
	assert.equal(out, "caught nope\n0 [10, 20, 30, 40, 50] 7\n");
	assert.deepEqual(order, [0, 5, 4, 3, 2, 1], "the fan-out ran concurrently");
});

test("the driver: fuel slices yield, and a budget stops a big operation before it starts", async () => {
	const s = rt.session();
	let yields = 0;
	const r = await run(s, "t = 0\nfor i in range(100000):\n    t += i\nt", {
		tools: {},
		fuelSlice: 10_000,
		yieldEvery: async () => {
			yields++;
		},
	});
	assert.ok(r.kind === "complete" && r.progress.completion?.repr === "4999950000");
	assert.ok(yields >= 5 && r.slices === yields + 1, `${yields} yields, ${r.slices} slices`);
	// Under the int work cap (7 ** 3000000 is past it: MemoryError at once), but
	// its cost is asked for before it starts.
	const b = await run(s, "x = 7 ** 300000", { tools: {}, fuelSlice: 10_000, fuelBudget: 50_000 });
	assert.equal(b.kind, "budget");
	assert.equal(s.phase(), "idle");
	// A native that charges after it runs overdraws its slice (each replace charges
	// about 8.5M units): the budget counts the charge, so the loop stops after three
	// replaces instead of running a hundred on 10,000-unit slices.
	assert.equal(s.start("s = 'ab' * 2_000_000\nn = 0").kind, "complete");
	const o = await run(s, "for i in range(100):\n    t = s.replace('a', 'xy')\n    n += 1", {
		tools: {},
		fuelSlice: 10_000,
		fuelBudget: 20_000_000,
	});
	assert.equal(o.kind, "budget");
	assert.ok(o.fuel >= 20_000_000 && o.fuel < 40_000_000, `${o.fuel} fuel`);
	// The third replace overdraws the budget; the cell stops at the next safepoint.
	const n = s.start("n");
	assert.ok(n.kind === "complete" && Number(n.completion?.repr) <= 3, JSON.stringify(n));
	assert.equal(s.fuelTotal(), null, "the driver leaves no fuel total behind");
});

test("the driver: a collection step ends a slice too, without fuel charged for it", async () => {
	const src =
		"keep = [[i] for i in range(50_000)]\nt = 0\nfor i in range(100_000):\n    b = [i, (i, str(i)), {'x': [i]}]\n    t += len(b)\nt + len(keep)";
	const sliced = await run(rt.session(), src, { tools: {}, fuelSlice: 10_000, yieldEvery: async () => {} });
	assert.ok(sliced.kind === "complete" && sliced.progress.completion?.repr === "350000");
	// The fuel of the cell run without slices, but more slices than fuel alone ends.
	const whole = rt.session();
	whole.setFuelTotal(2 ** 40);
	assert.equal(whole.start(src).kind, "complete");
	assert.equal(sliced.fuel, 2 ** 40 - (whole.fuelTotal() ?? 0));
	assert.ok(sliced.slices > Math.ceil(sliced.fuel / 10_000), `${sliced.slices} slices, ${sliced.fuel} fuel`);
});

test("K sessions per instance: placement, slot reuse, isolation, and a restore into a shared instance", async () => {
	const krt = await Runtime.create(guest("default"), { sessionsPerInstance: 4 });
	const tools: HostModule = { name: "t", functions: [{ name: "f", params: [{ name: "x" }] }] };
	const ss = Array.from({ length: 6 }, (_, i) => {
		const s = krt.session(options);
		s.define(tools);
		assert.equal(s.start(`import t\nme = ${i}`).kind, "complete");
		return s;
	});
	assert.deepEqual({ ...krt.stats(), memoryBytes: 0 }, { instances: 2, sessions: 6, memoryBytes: 0 });
	assert.equal(ss[0]?.memoryBytes(), ss[3]?.memoryBytes(), "the first four share an instance");
	// Two sessions of one instance stop mid-cell at a host call; a third runs whole
	// cells in between; output goes to each session's own callback.
	const [a, b, c] = ss as [Session, Session, Session];
	const outs = ["", "", ""];
	[a, b, c].forEach((s, i) => s.setOutput((_, t) => (outs[i] += t)));
	let pa = a.start("print('a', me)\nr = t.f(me)\nprint('a got', r)\nr");
	let pb = b.start("print('b', me)\nr = t.f(me * 10)\nr");
	assert.ok(pa.kind === "host-call" && pb.kind === "host-call");
	assert.deepEqual([pa.args, pb.args], [[0], [10]]);
	const pc = c.start("print('c', me)\nme * 100");
	assert.ok(pc.kind === "complete" && pc.completion?.repr === "200");
	// A park request belongs to its session only.
	b.requestPark();
	pa = a.resume({ kind: "value", value: "A" });
	assert.ok(pa.kind === "complete" && pa.completion?.repr === "'A'", JSON.stringify(pa));
	pb = b.resume({ kind: "value", value: "B" });
	if (pb.kind === "parked") pb = b.resume({ kind: "continue" });
	assert.ok(pb.kind === "complete" && pb.completion?.repr === "'B'", JSON.stringify(pb));
	assert.deepEqual(outs, ["a 0\na got A\n", "b 1\n", "c 2\n"]);
	// An instance is let go when its last session is disposed.
	for (const s of [ss[4], ss[5]]) s?.dispose();
	assert.deepEqual({ ...krt.stats(), memoryBytes: 0 }, { instances: 1, sessions: 4, memoryBytes: 0 });
	// A snapshot of a session mid-call restores into the slot it left.
	pb = b.start("t.f('x') + me");
	assert.equal(pb.kind, "host-call");
	const image = b.snapshot();
	b.dispose();
	assert.equal(krt.stats().sessions, 3);
	const r = krt.restore(image, options);
	assert.equal(r.memoryBytes(), a.memoryBytes());
	assert.deepEqual({ ...krt.stats(), memoryBytes: 0 }, { instances: 1, sessions: 4, memoryBytes: 0 });
	const pr = r.resume({ kind: "value", value: 41 });
	assert.ok(pr.kind === "complete" && pr.completion?.repr === "42", JSON.stringify(pr));
	for (const s of [a, c, ss[3], r]) s?.dispose();
	assert.deepEqual(krt.stats(), { instances: 0, sessions: 0, memoryBytes: 0 });
	// The default stays one session per instance.
	const one = [rt.session(), rt.session()];
	assert.equal(rt.stats().instances, rt.stats().sessions);
	for (const s of one) s.dispose();
});

test("K sessions per instance: a trap loses every session of that instance and no other", async () => {
	const krt = await Runtime.create(guest("default"), { sessionsPerInstance: 2 });
	const [a, b, c] = [krt.session(), krt.session(), krt.session()] as [Session, Session, Session];
	assert.equal(krt.stats().instances, 2);
	for (const s of [a, b, c]) assert.equal(s.start("x = 1").kind, "complete");
	// A host callback that throws unwinds through the guest: a trap.
	a.setOutput(() => {
		throw new Error("host bug");
	});
	assert.throws(() => a.start("print('boom')"), (e: unknown) => e instanceof MmError && e.kind === "trap");
	assert.deepEqual([a.isDead, b.isDead, c.isDead], [true, true, false]);
	assert.throws(() => b.start("x"), (e: unknown) => e instanceof MmError && e.kind === "trap");
	const pc = c.start("x + 1");
	assert.ok(pc.kind === "complete" && pc.completion?.repr === "2");
	// The dead instance takes no new sessions: the next one fills c's instance.
	const d = krt.session();
	assert.equal(d.memoryBytes(), c.memoryBytes());
	assert.equal(d.isDead, false);
	for (const s of [a, b, c, d]) s.dispose();
	assert.deepEqual(krt.stats(), { instances: 0, sessions: 0, memoryBytes: 0 });
});

test("past 2 GiB of linear memory, pointers stay unsigned: each session's bytes, entropy and output stay its own", async () => {
	// Pointers at or above 2^31 arrive from wasm as negative numbers; read as such they
	// passed the bounds checks and indexed from the end of memory, or threw a raw
	// RangeError (review J1). Three sessions in one instance hold about 780 MB each.
	// Then the entropy, output and input buffers below are big enough to be allocated
	// fresh, at the top of linear memory, above 2 GiB.
	const krt = await Runtime.create(guest(main), { sessionsPerInstance: 4 });
	const big = sessionOptions({ limits: { memory: 1 << 30, recursionDepth: 200, fuel: null } });
	const N = 60_000_000;
	const ss = [1, 2, 3].map((k) => {
		const s = krt.session(big);
		const p = s.start(`import os\nk = ${k}\nhold = [bytes([k]) * ${N} for _ in range(13)]\n`);
		assert.equal(p.kind, "complete", JSON.stringify(p));
		return s;
	});
	assert.ok(ss[0]?.memoryBytes() === ss[2]?.memoryBytes());
	assert.ok((ss[0]?.memoryBytes() ?? 0) > 2 ** 31, `${ss[0]?.memoryBytes()} bytes of linear memory`);
	const wide = "é".repeat(300_000);
	for (const [i, s] of ss.entries()) {
		let out = "";
		s.setOutput((_, t) => {
			out += t;
		});
		// A long cell, so that the input buffer grows into new memory too.
		const p = s.start(`# ${"x".repeat(64 << 10)}\nr = os.urandom(1_000_000)\nprint('session', k, 'é' * 300_000)\nr.count(0) < 10_000`);
		assert.ok(p.kind === "complete", JSON.stringify(p).slice(0, 400));
		assert.equal(p.completion?.repr, "True", `session ${i + 1}: its entropy is not zeros`);
		assert.ok(out === `session ${i + 1} ${wide}\n`, `session ${i + 1}: output ${JSON.stringify(out.slice(0, 40))}`);
		assert.equal(s.isDead, false);
	}
	// No session's held bytes were written by another's entropy or input.
	for (const [i, s] of ss.entries()) {
		const p = s.start(`ref = bytes([k]) * ${N}\nall(h == ref for h in hold)`);
		assert.ok(p.kind === "complete" && p.completion?.repr === "True", `session ${i + 1}: ${JSON.stringify(p).slice(0, 200)}`);
	}
	for (const s of ss) s.dispose();
	assert.deepEqual(krt.stats(), { instances: 0, sessions: 0, memoryBytes: 0 });
});
