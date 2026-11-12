/**
 * The `mm-wasm` codec (crates/mm-wasm-abi/src/lib.rs) in TypeScript: the
 * export and import names, the status and event codes, and the hand-written
 * little-endian encoding of what crosses the boundary. Every decoder refuses
 * an unknown tag with {@link Bad}, as the Rust one does.
 *
 * Values ({@link HostValue}) map onto JavaScript as Pyodide maps them, with
 * wrappers where JavaScript has no exact type:
 *
 * - `None` ↔ `null`, `bool` ↔ `boolean`, `str` ↔ `string`, `bytes` ↔ `Uint8Array`,
 *   `list` ↔ `Array`.
 * - `int` → a `number` when it is a safe integer, else a `bigint`; a `number`
 *   that is a safe integer (not `-0`) or a `bigint` → `int`.
 * - `float` → a `number`, or a {@link Float} when the value is integral (`2.0`), so
 *   that a round trip keeps the type; a non-integral `number` or a `Float` → `float`.
 * - `dict` → a `Map` (Python's order and keys, exactly); a `Map` or a plain object
 *   → `dict`.
 * - `tuple`, records, host-class objects and the unconverted forms are classes:
 *   {@link Tuple}, {@link PyRecord}, {@link PyObject}, {@link Unconverted}.
 *
 * Written in erasable TypeScript only, so Node runs it without a build step and
 * any bundler (Wrangler's esbuild) strips the types.
 */

export const ABI_VERSION = 1;
export const RESERVED = 0xff;
export const IMPORT_MODULE = "mm_host";
/** What the deadline import is compared against when no deadline is set. */
export const ARMED = (1n << 63n) - 2n;
export const MAX_DEPTH = 1000;
const U64_MAX = (1n << 64n) - 1n;

export const exportName = {
	ABI_VERSION: "mm_abi_version",
	BUILD_ID: "mm_build_id",
	INPUT: "mm_input",
	OUTPUT: "mm_output",
	SESSION_NEW: "mm_session_new",
	SESSION_RESTORE: "mm_session_restore",
	SESSION_DROP: "mm_session_drop",
	DEFINE: "mm_define",
	START: "mm_start",
	RESUME: "mm_resume",
	CANCEL: "mm_cancel",
	RESET: "mm_reset",
	SNAPSHOT: "mm_snapshot",
	SET_FUEL: "mm_set_fuel",
	FUEL: "mm_fuel",
	FUEL_WANTED: "mm_fuel_wanted",
	SET_FUEL_TOTAL: "mm_set_fuel_total",
	FUEL_TOTAL: "mm_fuel_total",
	PHASE: "mm_phase",
	SET_DEADLINE: "mm_set_deadline",
	SET_HINT: "mm_set_hint",
	SET_TEXT_CUT: "mm_set_text_cut",
	SET_LOCAL_TIMEZONE: "mm_set_local_timezone",
	CALL_LOG: "mm_call_log",
	ENVIRONMENT: "mm_environment",
	STUBS: "mm_stubs",
	HOST_MODULES: "mm_host_modules",
	SET_MODULE_DUNDERS: "mm_set_module_dunders",
	SET_INT_BITS_TEXT: "mm_set_int_bits_text",
	SET_GC_STRESS: "mm_set_gc_stress",
	SET_GC_YIELD: "mm_set_gc_yield",
	TEST_PANIC: "mm_test_panic",
	PENDING: "mm_pending",
} as const;

/** Every export a host may rely on (all but the optional `mm_set_gc_yield` and `mm_test_panic`). */
export const REQUIRED_EXPORTS: readonly string[] = [
	...Object.values(exportName).filter((n) => n !== exportName.TEST_PANIC && n !== exportName.SET_GC_YIELD),
	"memory",
];

export const importName = {
	NOW_NS: "now_ns",
	PARK: "park",
	WALL_CLOCK: "wall_clock",
	ENTROPY: "entropy",
	OUTPUT: "output",
	ZONE: "zone",
	/** Imported only by the bundle guest; served from its custom sections. */
	BUNDLE_READ: "bundle_read",
} as const;

/** The bundle guest's custom sections, by `mm_host.bundle_read` region. */
export const bundleSections = ["mm.prelude", "mm.stdlib"] as const;

export const event = {
	COMPLETE: 0,
	HOST_CALL: 1,
	WAITING: 2,
	PARKED: 3,
	ERROR: 4,
	COMPILE_ERROR: 5,
	RESUME_ERROR: 6,
	BAD_REQUEST: 7,
} as const;

export const status = { OK: 0, PY_ERROR: 1, SNAPSHOT_ERROR: 2, DEFINE_ERROR: 3, BAD_REQUEST: 7 } as const;

export type Phase = "idle" | "host-call" | "waiting" | "parked";
const PHASES: readonly Phase[] = ["idle", "host-call", "waiting", "parked"];

export function phaseFromCode(code: number): Phase {
	const p = PHASES[code];
	if (p === undefined) throw new Bad("phase");
	return p;
}

export type TextCut = "head" | "head-tail";
export const textCutCode = (cut: TextCut): number => (cut === "head-tail" ? 1 : 0);

/** Undecodable input; the message says what was wrong. */
export class Bad extends Error {
	constructor(what: string) {
		super(what);
		this.name = "Bad";
	}
}

// ------------------------------------------------------------------ values

/** A Python `float` whose value is integral (`2.0`), kept apart from `int`. */
export class Float {
	readonly value: number;
	constructor(value: number) {
		this.value = value;
	}
	valueOf(): number {
		return this.value;
	}
	toJSON(): number {
		return this.value;
	}
}

/** A Python `tuple`. */
export class Tuple {
	readonly items: HostValue[];
	constructor(items: HostValue[]) {
		this.items = items;
	}
}

/** A record (a dataclass or named tuple the engine converts by fields). */
export class PyRecord {
	readonly name: string;
	readonly fields: Map<string, HostValue>;
	constructor(name: string, fields: Map<string, HostValue>) {
		this.name = name;
		this.fields = fields;
	}
}

/** An instance of a host class (`HostModule::classes`). */
export class PyObject {
	readonly cls: string;
	readonly fields: Map<string, HostValue>;
	constructor(cls: string, fields: Map<string, HostValue>) {
		this.cls = cls;
		this.fields = fields;
	}
}

/** A value the engine could not convert (lenient host arguments). */
export class Unconverted {
	readonly kind: "opaque" | "key-not-str" | "too-deep" | "too-large";
	readonly typeName: string;
	readonly repr: string;
	constructor(kind: Unconverted["kind"], typeName = "", repr = "") {
		this.kind = kind;
		this.typeName = typeName;
		this.repr = repr;
	}
}

export type HostValue =
	| null
	| boolean
	| number
	| bigint
	| string
	| Uint8Array
	| Float
	| HostValue[]
	| Tuple
	| Map<string, HostValue>
	| { [key: string]: HostValue }
	| PyRecord
	| PyObject
	| Unconverted;

/** A decoded value as plain JSON-like JavaScript: maps and records become objects, tuples arrays, floats numbers. */
export function toJs(v: HostValue): unknown {
	if (v === null || typeof v !== "object") return v;
	if (v instanceof Float) return v.value;
	if (Array.isArray(v)) return v.map(toJs);
	if (v instanceof Tuple) return v.items.map(toJs);
	if (v instanceof Map) return mapToObject(v);
	if (v instanceof PyRecord || v instanceof PyObject) return mapToObject(v.fields);
	if (v instanceof Uint8Array || v instanceof Unconverted) return v;
	return v;
}

function mapToObject(m: Map<string, HostValue>): Record<string, unknown> {
	const o: Record<string, unknown> = {};
	for (const [k, x] of m) o[k] = toJs(x);
	return o;
}

// ---------------------------------------------------------------- the writer

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

/** A growable little-endian byte buffer. */
export class Writer {
	buf: Uint8Array;
	view: DataView;
	len = 0;
	constructor(capacity = 256) {
		this.buf = new Uint8Array(capacity);
		this.view = new DataView(this.buf.buffer);
	}
	reset(): this {
		this.len = 0;
		return this;
	}
	bytes(): Uint8Array {
		return this.buf.subarray(0, this.len);
	}
	private room(n: number): void {
		if (this.len + n <= this.buf.length) return;
		let cap = this.buf.length * 2;
		while (cap < this.len + n) cap *= 2;
		const next = new Uint8Array(cap);
		next.set(this.buf.subarray(0, this.len));
		this.buf = next;
		this.view = new DataView(next.buffer);
	}
	u8(v: number): void {
		this.room(1);
		this.buf[this.len++] = v;
	}
	bool(b: boolean): void {
		this.u8(b ? 1 : 0);
	}
	u32(v: number): void {
		this.room(4);
		this.view.setUint32(this.len, v, true);
		this.len += 4;
	}
	u64(v: bigint): void {
		this.room(8);
		this.view.setBigUint64(this.len, BigInt.asUintN(64, v), true);
		this.len += 8;
	}
	i64(v: bigint): void {
		this.room(8);
		this.view.setBigInt64(this.len, v, true);
		this.len += 8;
	}
	f64(v: number): void {
		this.room(8);
		this.view.setFloat64(this.len, v, true);
		this.len += 8;
	}
	/** An optional `u64`, `null` as `u64::MAX`. */
	optU64(v: bigint | number | null | undefined): void {
		this.u64(v === null || v === undefined ? U64_MAX : BigInt(v));
	}
	raw(b: Uint8Array): void {
		this.room(b.length);
		this.buf.set(b, this.len);
		this.len += b.length;
	}
	byteString(b: Uint8Array): void {
		this.u32(b.length);
		this.raw(b);
	}
	str(s: string): void {
		// Up to 3 bytes per UTF-16 unit.
		this.room(4 + s.length * 3);
		const at = this.len + 4;
		const { written } = encoder.encodeInto(s, this.buf.subarray(at));
		this.view.setUint32(this.len, written, true);
		this.len = at + written;
	}
	optStr(s: string | null | undefined): void {
		this.bool(s !== null && s !== undefined);
		if (s !== null && s !== undefined) this.str(s);
	}
	optBytes(b: Uint8Array | null | undefined): void {
		this.bool(b !== null && b !== undefined);
		if (b !== null && b !== undefined) this.byteString(b);
	}
}

const I64_MIN = -(1n << 63n);
const I64_MAX = (1n << 63n) - 1n;

function putInt(w: Writer, i: bigint): void {
	if (i >= I64_MIN && i <= I64_MAX) {
		w.u8(3);
		w.i64(i);
	} else {
		w.u8(4);
		w.str(i.toString());
	}
}

function putFields(w: Writer, fields: Map<string, HostValue> | { [key: string]: HostValue }, depth: number): void {
	const entries = fields instanceof Map ? [...fields] : Object.entries(fields);
	w.u32(entries.length);
	for (const [k, v] of entries) {
		w.str(k);
		putValueAt(w, v, depth);
	}
}

/** Appends a value ([`put_value`]). */
export function putValue(w: Writer, v: HostValue): void {
	putValueAt(w, v, 0);
}

function putValueAt(w: Writer, v: HostValue, depth: number): void {
	if (depth > MAX_DEPTH) throw new Bad("value nested too deep");
	switch (typeof v) {
		case "boolean":
			w.u8(v ? 2 : 1);
			return;
		case "number":
			if (Number.isSafeInteger(v) && !Object.is(v, -0)) putInt(w, BigInt(v));
			else {
				w.u8(5);
				w.f64(v);
			}
			return;
		case "bigint":
			putInt(w, v);
			return;
		case "string":
			w.u8(6);
			w.str(v);
			return;
		case "object":
			break;
		default:
			throw new Bad(`not a host value: ${typeof v}`);
	}
	if (v === null) return w.u8(0);
	if (Array.isArray(v) || v instanceof Tuple) {
		const items = Array.isArray(v) ? v : v.items;
		w.u8(Array.isArray(v) ? 8 : 9);
		w.u32(items.length);
		for (const x of items) putValueAt(w, x, depth + 1);
		return;
	}
	if (v instanceof Float) {
		w.u8(5);
		w.f64(v.value);
		return;
	}
	if (v instanceof Uint8Array) {
		w.u8(7);
		w.byteString(v);
		return;
	}
	if (v instanceof Map) {
		w.u8(10);
		putFields(w, v, depth + 1);
		return;
	}
	if (v instanceof PyRecord || v instanceof PyObject) {
		w.u8(v instanceof PyRecord ? 11 : 12);
		w.str(v instanceof PyRecord ? v.name : v.cls);
		putFields(w, v.fields, depth + 1);
		return;
	}
	if (v instanceof Unconverted) {
		switch (v.kind) {
			case "opaque":
				w.u8(13);
				w.str(v.typeName);
				w.str(v.repr);
				return;
			case "key-not-str":
				return w.u8(14);
			case "too-deep":
				return w.u8(15);
			case "too-large":
				w.u8(16);
				w.str(v.typeName);
				return;
		}
	}
	const proto = Object.getPrototypeOf(v);
	if (proto === Object.prototype || proto === null) {
		w.u8(10);
		putFields(w, v as { [key: string]: HostValue }, depth + 1);
		return;
	}
	throw new Bad(`not a host value: ${proto?.constructor?.name ?? "object"}`);
}

function putOptValue(w: Writer, v: HostValue | undefined): void {
	w.bool(v !== undefined);
	if (v !== undefined) putValue(w, v);
}

// ---------------------------------------------------------------- the reader

/** A cursor over input; strings are decoded straight from it. */
export class Reader {
	readonly data: Uint8Array;
	readonly view: DataView;
	at = 0;
	constructor(data: Uint8Array) {
		this.data = data;
		this.view = new DataView(data.buffer, data.byteOffset, data.byteLength);
	}
	get left(): number {
		return this.data.length - this.at;
	}
	take(n: number): Uint8Array {
		if (this.left < n) throw new Bad("truncated");
		const out = this.data.subarray(this.at, this.at + n);
		this.at += n;
		return out;
	}
	u8(): number {
		if (this.left < 1) throw new Bad("truncated");
		return this.data[this.at++] as number;
	}
	bool(): boolean {
		const b = this.u8();
		if (b > 1) throw new Bad("bool");
		return b === 1;
	}
	u32(): number {
		if (this.left < 4) throw new Bad("truncated");
		const v = this.view.getUint32(this.at, true);
		this.at += 4;
		return v;
	}
	u64(): bigint {
		if (this.left < 8) throw new Bad("truncated");
		const v = this.view.getBigUint64(this.at, true);
		this.at += 8;
		return v;
	}
	/** A `u64` that must fit a safe JavaScript integer (call ids). */
	id(): number {
		const v = this.u64();
		if (v > BigInt(Number.MAX_SAFE_INTEGER)) throw new Bad("id past 2^53");
		return Number(v);
	}
	i64(): bigint {
		if (this.left < 8) throw new Bad("truncated");
		const v = this.view.getBigInt64(this.at, true);
		this.at += 8;
		return v;
	}
	f64(): number {
		if (this.left < 8) throw new Bad("truncated");
		const v = this.view.getFloat64(this.at, true);
		this.at += 8;
		return v;
	}
	/** A count of items of at least `min` bytes each, refused when the input cannot hold them. */
	count(min: number): number {
		const n = this.u32();
		if (n * Math.max(min, 1) > this.left) throw new Bad("count");
		return n;
	}
	byteString(): Uint8Array {
		return this.take(this.u32());
	}
	string(): string {
		const b = this.byteString();
		try {
			return decoder.decode(b);
		} catch {
			throw new Bad("utf-8");
		}
	}
	optString(): string | undefined {
		return this.bool() ? this.string() : undefined;
	}
}

function intValue(i: bigint): number | bigint {
	return i >= -9007199254740991n && i <= 9007199254740991n ? Number(i) : i;
}

function getFields(r: Reader, depth: number): Map<string, HostValue> {
	const n = r.count(5);
	const m = new Map<string, HostValue>();
	for (let i = 0; i < n; i++) {
		const k = r.string();
		m.set(k, getValueAt(r, depth));
	}
	return m;
}

/** A value ([`get_value`]). */
export function getValue(r: Reader): HostValue {
	return getValueAt(r, 0);
}

function getValueAt(r: Reader, depth: number): HostValue {
	if (depth > MAX_DEPTH + 1) throw new Bad("value nested too deep");
	const tag = r.u8();
	switch (tag) {
		case 0:
			return null;
		case 1:
			return false;
		case 2:
			return true;
		case 3:
			return intValue(r.i64());
		case 4: {
			const text = r.string();
			try {
				return intValue(BigInt(text));
			} catch {
				throw new Bad("big int");
			}
		}
		case 5: {
			const f = r.f64();
			return Number.isInteger(f) ? new Float(f) : f;
		}
		case 6:
			return r.string();
		case 7:
			return r.byteString().slice();
		case 8:
		case 9: {
			const n = r.count(1);
			const items: HostValue[] = new Array(n);
			for (let i = 0; i < n; i++) items[i] = getValueAt(r, depth + 1);
			return tag === 8 ? items : new Tuple(items);
		}
		case 10:
			return getFields(r, depth + 1);
		case 11:
		case 12: {
			const name = r.string();
			const fields = getFields(r, depth + 1);
			return tag === 11 ? new PyRecord(name, fields) : new PyObject(name, fields);
		}
		case 13: {
			const typeName = r.string();
			return new Unconverted("opaque", typeName, r.string());
		}
		case 14:
			return new Unconverted("key-not-str");
		case 15:
			return new Unconverted("too-deep");
		case 16:
			return new Unconverted("too-large", r.string());
		default:
			throw new Bad("value tag");
	}
}

function getOptValue(r: Reader): HostValue | undefined {
	return r.bool() ? getValue(r) : undefined;
}

// ------------------------------------------------------------------ sessions

export interface Limits {
	/** Arena bytes; default 256 MiB. */
	memory: number;
	/** Default 1000. */
	recursionDepth: number;
	/** Fuel per cell; `null` is unlimited (the default). */
	fuel: number | null;
}

export interface HostArgs {
	/** Default 16 MiB. */
	maxBytes: number;
	/** Default 1000. */
	maxDepth: number;
	lenient: boolean;
}

/**
 * What `mm_session_new` and `mm_session_restore` take (`mm_wasm_abi::SessionOptions`),
 * as an options record ({@link putOptions}). A restored session takes the limits and
 * the host-side settings from it, and the text limit, `lastExprValue` and `hostArgs`
 * from the image.
 */
export interface SessionOptions {
	limits: Limits;
	/** `Session::set_text_limit`; `null` keeps the default (1 MiB, or the memory limit if less). New sessions only. */
	textLimit: number | null;
	/** New sessions only. */
	lastExprValue: boolean;
	/** New sessions only. */
	hostArgs: HostArgs;
	/** Install `mm_host.wall_clock`; without it `time.time()` raises `RuntimeError`. */
	wallClock: boolean;
	/** Install `mm_host.entropy`; without it `os.urandom` raises `RuntimeError`. */
	entropy: boolean;
	/** Install `mm_host.zone` (`zoneinfo`). */
	zoneSource: boolean;
	/** `Session::set_text_cut`. */
	textCut: TextCut;
	/** `Session::set_int_bits_text`. */
	intBitsText: boolean;
	/** `Session::set_gc_stress` (engine testing). */
	gcStress: boolean;
}

export const defaultLimits = (): Limits => ({ memory: 256 << 20, recursionDepth: 1000, fuel: null });

export function sessionOptions(o: Partial<Omit<SessionOptions, "limits" | "hostArgs">> & {
	limits?: Partial<Limits>;
	hostArgs?: Partial<HostArgs>;
} = {}): SessionOptions {
	return {
		limits: { ...defaultLimits(), ...o.limits },
		textLimit: o.textLimit ?? null,
		lastExprValue: o.lastExprValue ?? true,
		hostArgs: { maxBytes: 16 << 20, maxDepth: MAX_DEPTH, lenient: false, ...o.hostArgs },
		wallClock: o.wallClock ?? true,
		entropy: o.entropy ?? true,
		zoneSource: o.zoneSource ?? true,
		textCut: o.textCut ?? "head",
		intBitsText: o.intBitsText ?? false,
		gcStress: o.gcStress ?? false,
	};
}

/**
 * The options record's tags (`mm_wasm_abi::option`). Each names one option with an
 * encoding and a default that never change; a guest refuses a record with a tag it
 * does not know unless the tag is at least `IGNORABLE`.
 */
export const option = {
	IGNORABLE: 0x80,
	MEMORY: 0x01,
	RECURSION_DEPTH: 0x02,
	FUEL: 0x03,
	TEXT_LIMIT: 0x04,
	LAST_EXPR_VALUE: 0x05,
	HOST_ARGS_MAX_BYTES: 0x06,
	HOST_ARGS_MAX_DEPTH: 0x07,
	HOST_ARGS_LENIENT: 0x08,
	WALL_CLOCK: 0x09,
	ENTROPY: 0x0a,
	ZONE_SOURCE: 0x0b,
	TEXT_CUT: 0x0c,
	INT_BITS_TEXT: 0x0d,
	GC_STRESS: 0x0e,
} as const;

/**
 * Writes the options record: a `u32` byte length, then an entry for each option that
 * differs from its default, in tag order: the tag, a `u32` payload length, the payload.
 */
export function putOptions(w: Writer, o: SessionOptions): void {
	const d = sessionOptions();
	const e = new Writer(64);
	const entry = (tag: number, payload: (p: Writer) => void) => {
		const p = new Writer(8);
		payload(p);
		e.u8(tag);
		e.byteString(p.bytes());
	};
	if (o.limits.memory !== d.limits.memory) entry(option.MEMORY, (p) => p.u64(BigInt(o.limits.memory)));
	if (o.limits.recursionDepth !== d.limits.recursionDepth) {
		entry(option.RECURSION_DEPTH, (p) => p.u32(o.limits.recursionDepth));
	}
	if (o.limits.fuel !== null) entry(option.FUEL, (p) => p.optU64(o.limits.fuel));
	if (o.textLimit !== null) entry(option.TEXT_LIMIT, (p) => p.u64(BigInt(o.textLimit as number)));
	if (o.lastExprValue !== d.lastExprValue) entry(option.LAST_EXPR_VALUE, (p) => p.bool(o.lastExprValue));
	if (o.hostArgs.maxBytes !== d.hostArgs.maxBytes) {
		entry(option.HOST_ARGS_MAX_BYTES, (p) => p.u64(BigInt(o.hostArgs.maxBytes)));
	}
	if (o.hostArgs.maxDepth !== d.hostArgs.maxDepth) entry(option.HOST_ARGS_MAX_DEPTH, (p) => p.u32(o.hostArgs.maxDepth));
	if (o.hostArgs.lenient !== d.hostArgs.lenient) entry(option.HOST_ARGS_LENIENT, (p) => p.bool(o.hostArgs.lenient));
	if (o.wallClock !== d.wallClock) entry(option.WALL_CLOCK, (p) => p.bool(o.wallClock));
	if (o.entropy !== d.entropy) entry(option.ENTROPY, (p) => p.bool(o.entropy));
	if (o.zoneSource !== d.zoneSource) entry(option.ZONE_SOURCE, (p) => p.bool(o.zoneSource));
	if (o.textCut !== d.textCut) entry(option.TEXT_CUT, (p) => p.u8(textCutCode(o.textCut)));
	if (o.intBitsText !== d.intBitsText) entry(option.INT_BITS_TEXT, (p) => p.bool(o.intBitsText));
	if (o.gcStress !== d.gcStress) entry(option.GC_STRESS, (p) => p.bool(o.gcStress));
	w.byteString(e.bytes());
}

// -------------------------------------------------------------------- define

export type ParamKind = "positional-or-keyword" | "keyword-only" | "var-positional" | "var-keyword";
const PARAM_KINDS: readonly ParamKind[] = ["positional-or-keyword", "keyword-only", "var-positional", "var-keyword"];

export interface HostParam {
	name: string;
	annotation?: string;
	/** `undefined`: required. */
	default?: HostValue;
	kind?: ParamKind;
}

export interface HostSignature {
	name: string;
	params: HostParam[];
	returns?: string;
	doc?: string;
	/** A single dict argument binds as keyword arguments (`await tool({...})`). */
	dictAsKwargs?: boolean;
}

export interface HostClass {
	name: string;
	fields: [string, string][];
	methods: HostSignature[];
	doc?: string;
}

export interface HostException {
	name: string;
	/** The base's `HostErrorKind` name, such as `RuntimeError`. */
	base: string;
	doc?: string;
}

/** `micro_monty::HostModule`; `name` `""` is the builtins module. */
export interface HostModule {
	name: string;
	functions: HostSignature[];
	classes?: HostClass[];
	constants?: [string, HostValue][];
	exceptions?: HostException[];
}

/** `HostSignature::any(name)`: `*args, **kwargs`, untyped. */
export function anySignature(name: string): HostSignature {
	return {
		name,
		params: [
			{ name: "args", kind: "var-positional" },
			{ name: "kwargs", kind: "var-keyword" },
		],
	};
}

/** `HostModule::untyped(name, functions)`. */
export function untypedModule(name: string, functions: string[]): HostModule {
	return { name, functions: functions.map(anySignature) };
}

function putSignature(w: Writer, s: HostSignature): void {
	w.str(s.name);
	w.u32(s.params.length);
	for (const p of s.params) {
		w.str(p.name);
		w.str(p.annotation ?? "");
		putOptValue(w, p.default);
		const kind = PARAM_KINDS.indexOf(p.kind ?? "positional-or-keyword");
		if (kind < 0) throw new Bad("param kind");
		w.u8(kind);
	}
	w.str(s.returns ?? "");
	w.str(s.doc ?? "");
	w.bool(s.dictAsKwargs ?? false);
}

function getSignature(r: Reader): HostSignature {
	const name = r.string();
	const n = r.count(10);
	const params: HostParam[] = [];
	for (let i = 0; i < n; i++) {
		const pname = r.string();
		const annotation = r.string();
		const def = getOptValue(r);
		const kind = PARAM_KINDS[r.u8()];
		if (kind === undefined) throw new Bad("param kind");
		const p: HostParam = { name: pname, annotation, kind };
		if (def !== undefined) p.default = def;
		params.push(p);
	}
	return { name, params, returns: r.string(), doc: r.string(), dictAsKwargs: r.bool() };
}

export function putModule(w: Writer, m: HostModule): void {
	w.str(m.name);
	w.u32(m.functions.length);
	for (const f of m.functions) putSignature(w, f);
	const classes = m.classes ?? [];
	w.u32(classes.length);
	for (const c of classes) {
		w.str(c.name);
		w.u32(c.fields.length);
		for (const [k, a] of c.fields) {
			w.str(k);
			w.str(a);
		}
		w.u32(c.methods.length);
		for (const f of c.methods) putSignature(w, f);
		w.str(c.doc ?? "");
	}
	putFields(w, new Map(m.constants ?? []), 0);
	const exceptions = m.exceptions ?? [];
	w.u32(exceptions.length);
	for (const e of exceptions) {
		w.str(e.name);
		w.str(e.base);
		w.str(e.doc ?? "");
	}
}

export function getModule(r: Reader): HostModule {
	const name = r.string();
	let n = r.count(17);
	const functions: HostSignature[] = [];
	for (let i = 0; i < n; i++) functions.push(getSignature(r));
	n = r.count(16);
	const classes: HostClass[] = [];
	for (let i = 0; i < n; i++) {
		const cname = r.string();
		let k = r.count(8);
		const fields: [string, string][] = [];
		for (let j = 0; j < k; j++) fields.push([r.string(), r.string()]);
		k = r.count(17);
		const methods: HostSignature[] = [];
		for (let j = 0; j < k; j++) methods.push(getSignature(r));
		classes.push({ name: cname, fields, methods, doc: r.string() });
	}
	const constants = [...getFields(r, 0)];
	n = r.count(12);
	const exceptions: HostException[] = [];
	for (let i = 0; i < n; i++) exceptions.push({ name: r.string(), base: r.string(), doc: r.string() });
	return { name, functions, classes, constants, exceptions };
}

export interface HostFunctionRef {
	name: string;
	id: number;
}

export function getFunctionRefs(r: Reader): HostFunctionRef[] {
	const n = r.count(8);
	const out: HostFunctionRef[] = [];
	for (let i = 0; i < n; i++) out.push({ name: r.string(), id: r.u32() });
	return out;
}

export function getModules(r: Reader): HostModule[] {
	const n = r.count(20);
	const out: HostModule[] = [];
	for (let i = 0; i < n; i++) out.push(getModule(r));
	return out;
}

export function getStubs(r: Reader): [string, string][] {
	const n = r.count(8);
	const out: [string, string][] = [];
	for (let i = 0; i < n; i++) out.push([r.string(), r.string()]);
	return out;
}

export type CallStatus = "returned" | "raised" | "pending" | "abandoned";
const CALL_STATUS: readonly CallStatus[] = ["returned", "raised", "pending", "abandoned"];

export interface HostCallRecord {
	call: number;
	function: number;
	name: string;
	status: CallStatus;
}

export function getCallLog(r: Reader): HostCallRecord[] {
	const n = r.count(17);
	const out: HostCallRecord[] = [];
	for (let i = 0; i < n; i++) {
		const call = r.id();
		const fn = r.u32();
		const name = r.string();
		const st = CALL_STATUS[r.u8()];
		if (st === undefined) throw new Bad("call status");
		out.push({ call, function: fn, name, status: st });
	}
	return out;
}

// ------------------------------------------------------------ resume, progress

/** A failed host call: raised as `kind(message)`, or as the host exception class `cls`. */
export interface HostError {
	/** A `HostErrorKind` name (`Exception`, `ValueError`, `RuntimeError`, ...). */
	kind: string;
	message: string;
	data?: HostValue;
	/** A host exception class by its registered name (`module.Name`). */
	cls?: string;
}

export function putHostError(w: Writer, e: HostError): void {
	w.str(e.kind);
	w.str(e.message);
	putOptValue(w, e.data);
	w.optStr(e.cls);
}

export type Outcome = { ok: true; value: HostValue } | { ok: false; error: HostError };

export type Resume =
	| { kind: "continue" }
	| { kind: "value"; value: HostValue }
	| { kind: "error"; error: HostError }
	| { kind: "future" }
	| { kind: "batch"; items: [call: number, outcome: Outcome][] };

export function putResume(w: Writer, resume: Resume): void {
	switch (resume.kind) {
		case "continue":
			return w.u8(0);
		case "value":
			w.u8(1);
			return putValue(w, resume.value);
		case "error":
			w.u8(2);
			return putHostError(w, resume.error);
		case "future":
			return w.u8(3);
		case "batch":
			w.u8(4);
			w.u32(resume.items.length);
			for (const [call, o] of resume.items) {
				w.u64(BigInt(call));
				if (o.ok) {
					w.u8(0);
					putValue(w, o.value);
				} else {
					w.u8(1);
					putHostError(w, o.error);
				}
			}
			return;
		default:
			throw new Bad("resume kind");
	}
}

export interface PyException {
	typeName: string;
	message: string;
	traceback: string;
	truncated: boolean;
}

export function getException(r: Reader): PyException {
	return { typeName: r.string(), message: r.string(), traceback: r.string(), truncated: r.bool() };
}

export interface CompileError {
	typeName: string;
	message: string;
	line: number;
	column: number;
	rendered: string;
}

export function getCompileError(r: Reader): CompileError {
	return { typeName: r.string(), message: r.string(), line: r.u32(), column: r.u32(), rendered: r.string() };
}

export type ResumeError =
	| { kind: "unexpected"; phase: Phase }
	| { kind: "unknown-call"; call: number }
	| { kind: "already-settled"; call: number };

export function getResumeError(r: Reader): ResumeError {
	switch (r.u8()) {
		case 0:
			return { kind: "unexpected", phase: phaseFromCode(r.u8()) };
		case 1:
			return { kind: "unknown-call", call: r.id() };
		case 2:
			return { kind: "already-settled", call: r.id() };
		default:
			throw new Bad("resume error tag");
	}
}

export type SnapshotError =
	| { kind: "incompatible"; format: number; buildId: Uint8Array }
	| { kind: "corrupt"; what: string }
	| { kind: "too-large" }
	| { kind: "busy" }
	| { kind: "poisoned" }
	| { kind: "internal"; message: string };

export function getSnapshotError(r: Reader): SnapshotError {
	switch (r.u8()) {
		case 0: {
			const format = r.u32();
			if (format > 0xffff) throw new Bad("snapshot format");
			return { kind: "incompatible", format, buildId: r.take(32).slice() };
		}
		case 1:
			return { kind: "corrupt", what: r.string() };
		case 2:
			return { kind: "too-large" };
		case 3:
			return { kind: "busy" };
		case 4:
			return { kind: "poisoned" };
		case 5:
			return { kind: "internal", message: r.string() };
		default:
			throw new Bad("snapshot error tag");
	}
}

export type DefineError =
	| { kind: "invalid"; text: string }
	| { kind: "busy" }
	| { kind: "raised"; error: PyException }
	| { kind: "poisoned"; text: string };

export function getDefineError(r: Reader): DefineError {
	switch (r.u8()) {
		case 0:
			return { kind: "invalid", text: r.string() };
		case 1:
			return { kind: "busy" };
		case 2:
			return { kind: "raised", error: getException(r) };
		case 3:
			return { kind: "poisoned", text: r.string() };
		default:
			throw new Bad("define error tag");
	}
}

export interface Completion {
	repr: string;
	str: string;
	typeName: string;
	reprTruncated: boolean;
	strTruncated: boolean;
	/** The value as plain data when it converts within the session's host-argument limits. */
	value?: HostValue;
}

export type ParkReason = "fuel" | "requested" | "deadline";
const PARK_REASONS: readonly ParkReason[] = ["fuel", "requested", "deadline"];

export type Progress =
	| { kind: "complete"; completion: Completion | null }
	| {
			kind: "host-call";
			call: number;
			function: number;
			name: string;
			args: HostValue[];
			kwargs: Map<string, HostValue>;
	  }
	| { kind: "waiting"; calls: number[] }
	| { kind: "parked"; reason: ParkReason }
	| { kind: "error"; error: PyException };

/** The inverse of `put_progress` for the progress events. */
export function getProgress(code: number, r: Reader): Progress {
	switch (code) {
		case event.COMPLETE: {
			if (!r.bool()) return { kind: "complete", completion: null };
			const c: Completion = {
				repr: r.string(),
				str: r.string(),
				typeName: r.string(),
				reprTruncated: r.bool(),
				strTruncated: r.bool(),
			};
			const value = getOptValue(r);
			if (value !== undefined) c.value = value;
			return { kind: "complete", completion: c };
		}
		case event.HOST_CALL: {
			const call = r.id();
			const fn = r.u32();
			const name = r.string();
			const n = r.count(1);
			const args: HostValue[] = new Array(n);
			for (let i = 0; i < n; i++) args[i] = getValue(r);
			return { kind: "host-call", call, function: fn, name, args, kwargs: getFields(r, 0) };
		}
		case event.WAITING: {
			const n = r.count(8);
			const calls: number[] = new Array(n);
			for (let i = 0; i < n; i++) calls[i] = r.id();
			return { kind: "waiting", calls };
		}
		case event.PARKED: {
			const reason = PARK_REASONS[r.u8()];
			if (reason === undefined) throw new Bad("park reason");
			return { kind: "parked", reason };
		}
		case event.ERROR:
			return { kind: "error", error: getException(r) };
		default:
			throw new Bad("not a progress event");
	}
}

/** The build id an image carries (bytes 10..42, after the magic and the format), or `undefined`. */
export function imageBuildId(image: Uint8Array): Uint8Array | undefined {
	if (image.length < 42) return undefined;
	for (let i = 0; i < 8; i++) if (image[i] !== SNAPSHOT_MAGIC[i]) return undefined;
	return image.subarray(10, 42);
}

/** `micro_monty`'s image magic, `b"MMSNAP\0\x01"`. */
const SNAPSHOT_MAGIC = [0x4d, 0x4d, 0x53, 0x4e, 0x41, 0x50, 0x00, 0x01];

export function hex(b: Uint8Array): string {
	let s = "";
	for (const x of b) s += x.toString(16).padStart(2, "0");
	return s;
}
