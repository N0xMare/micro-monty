/**
 * A JavaScript host for the `mm-wasm` guest (crates/mm-wasm-guest) on any
 * runtime with WebAssembly: Node, Deno, Bun, browsers, and Cloudflare Workers
 * and Durable Objects (workerd). It follows the reference wasmtime host
 * (crates/mm-wasm-host/src/lib.rs) call for call, and the API mirrors
 * `micro_monty::Session`. By default one {@link Session} owns one instance
 * (owner decision 2026-11-05), so a trap loses that session only;
 * {@link RuntimeConfig.sessionsPerInstance} opts into K sessions per instance,
 * which saves memory and shares traps.
 *
 * Every call is synchronous, as the reference host's are: a host call is a
 * stop (`host-call`, `waiting`), never a re-entry, so an async tool is
 * answered with `{ kind: "future" }` and later with a batch. `driver.ts` is the
 * async loop a code-mode host runs on top.
 *
 * What differs from wasmtime, because V8 has no epoch interruption and no
 * store limits:
 *
 * - No grace interrupt. A park request ({@link Session.requestPark}, or a
 *   shared `Int32Array` another thread writes, {@link RuntimeConfig.parkSignal})
 *   is polled by the guest every 256 safepoints and parks there without a trap,
 *   as under wasmtime; but one long native operation that reaches no safepoint
 *   cannot be stopped from JavaScript. Run cells in fuel slices and check
 *   {@link Session.fuelWanted} before granting more (as the reference host
 *   documents), and rely on the platform's own limit (a Node worker's
 *   `terminate()`, the Workers CPU limit) as the last resort.
 * - Deadlines read `performance.now()`. On Cloudflare Workers the clock does
 *   not advance while JavaScript (or wasm) runs synchronously, so a deadline
 *   only fires after the request did I/O; use fuel there.
 * - No store memory limit: the session's `limits.memory` (the arena) is the
 *   cap; linear memory grows past it only by the engine's out-of-arena
 *   buffers. {@link Session.memoryBytes} reports it.
 */

import {
	ABI_VERSION,
	Bad,
	bundleSections,
	type CompileError,
	type DefineError,
	event,
	exportName,
	getCallLog,
	getCompileError,
	getDefineError,
	getException,
	getFunctionRefs,
	getModules,
	getProgress,
	getResumeError,
	getSnapshotError,
	getStubs,
	type HostCallRecord,
	type HostFunctionRef,
	type HostModule,
	hex,
	IMPORT_MODULE,
	imageBuildId,
	importName,
	type Phase,
	type Progress,
	type PyException,
	phaseFromCode,
	putModule,
	putOptions,
	putResume,
	REQUIRED_EXPORTS,
	Reader,
	type Resume,
	type ResumeError,
	type SessionOptions,
	type SnapshotError,
	sessionOptions,
	status,
	type TextCut,
	textCutCode,
	Writer,
} from "./abi.ts";

export * from "./abi.ts";

export type MmErrorKind = "trap" | "compile" | "resume" | "py" | "define" | "snapshot" | "abi" | "unknown-build";

/** Why a call failed (`mm_wasm_host::Error`). */
export class MmError extends Error {
	readonly kind: MmErrorKind;
	/** The decoded detail: a `CompileError`, `ResumeError`, `PyException`, `DefineError`, `SnapshotError` or build id. */
	readonly detail: CompileError | ResumeError | PyException | DefineError | SnapshotError | Uint8Array | undefined;
	constructor(kind: MmErrorKind, message: string, detail?: MmError["detail"]) {
		super(message);
		this.name = "MmError";
		this.kind = kind;
		this.detail = detail;
	}
}

export type Stream = "stdout" | "stderr";
export type Output = (stream: Stream, text: string) => void;
/** `zoneinfo.ZoneInfo(key)`'s TZif bytes, or `undefined` for an unknown key. */
export type ZoneSource = (key: string) => Uint8Array | undefined;

export interface RuntimeConfig {
	/**
	 * A park flag another thread can set (`Atomics.store(flag, 0, 1)`), for Node
	 * worker threads: every session of this runtime reads index 0 and clears it when
	 * it parks. Default: none; {@link Session.requestPark} works either way.
	 */
	parkSignal?: Int32Array;
	/** The monotonic clock deadlines use, in ms. Default `performance.now()`. */
	now?: () => number;
	/** `time.time()`, in seconds since the epoch. Default `Date.now() / 1000`. */
	wallClock?: () => number;
	/** Randomness for `os.urandom` and unseeded `random`. Default `crypto.getRandomValues`. */
	entropy?: (buf: Uint8Array) => void;
	/**
	 * How many sessions one wasm instance holds (K). Default 1: every session has an
	 * instance of its own, as the reference host does. Above 1, a new or restored
	 * session goes into a live instance of its build with a free slot, and a new
	 * instance starts only when none has one. That shares the instance's fixed part
	 * (the shadow stack, the data segments, the allocator's slack) and costs:
	 *
	 * - A trap loses every session in the instance, not one.
	 * - The instance's linear memory must hold the sum of its sessions' bounds, and
	 *   it never shrinks while any of them lives. A wasm32 memory ends at 4 GiB, so
	 *   K × the per-session bound (about 3 × `limits.memory` + 16 MiB,
	 *   docs/sdk.md) must fit under it, and under the platform's limit (128 MB per
	 *   isolate on Workers).
	 * - One session's long synchronous call blocks the others; it would anyway,
	 *   since JavaScript runs one call at a time.
	 */
	sessionsPerInstance?: number;
}

type Fn = (...args: never[]) => unknown;
type Required = Exclude<(typeof exportName)[keyof typeof exportName], "mm_test_panic" | "mm_set_gc_yield">;
type Exports = { [K in Required]: Fn } & {
	memory: WebAssembly.Memory;
	mm_set_gc_yield?: Fn;
	mm_test_panic?: Fn;
	_initialize?: Fn;
};

interface Guest {
	module: WebAssembly.Module;
	buildId: Uint8Array;
	key: string;
}

const U64_MAX = (1n << 64n) - 1n;
const u64 = (v: number | bigint | null | undefined): bigint => (v === null || v === undefined ? U64_MAX : BigInt(v));
// The guest's u64 results arrive as signed i64 BigInts: u64::MAX is -1n.
const unsigned = (v: bigint): bigint => BigInt.asUintN(64, v);
const optNumber = (v: bigint): number | null => (unsigned(v) === U64_MAX ? null : Number(unsigned(v)));
// Pointers, lengths and offsets are wasm32 `u32`s, but V8 hands every i32 to
// JavaScript as a signed number: past 2 GiB of linear memory a pointer would read
// as negative, pass `ptr + len <= m.length`, and index the wrong bytes.
const u32 = (v: number): number => v >>> 0;

/**
 * The loaded guest modules, keyed by build id; shared by all sessions. New sessions
 * run on the current (last loaded) module; {@link Runtime.restore} picks the module
 * whose build wrote the image, so an upgrading host keeps the old module loaded
 * until its parked snapshots drained (docs/sdk.md, "Upgrading: drain old builds").
 */
export class Runtime {
	readonly config: RuntimeConfig;
	private guests: Guest[] = [];
	/** Instances that hold sessions. */
	private live = new Set<Instance>();
	/** Instances with a free session slot, by build (only with `sessionsPerInstance` above 1). */
	private open = new Map<string, Instance[]>();
	/** The last {@link Runtime.load}'s compile time in ms (0 for a module compiled by the caller). */
	loadMs = 0;

	private constructor(config: RuntimeConfig) {
		this.config = config;
	}

	/** A runtime with `module` as its current guest: a compiled `WebAssembly.Module` (Workers, where bytes cannot be compiled at run time) or the module's bytes. */
	static async create(module: WebAssembly.Module | BufferSource, config: RuntimeConfig = {}): Promise<Runtime> {
		const rt = new Runtime(config);
		await rt.load(module);
		return rt;
	}

	/** {@link Runtime.create} for an already compiled module, without a promise. */
	static fromModule(module: WebAssembly.Module, config: RuntimeConfig = {}): Runtime {
		const rt = new Runtime(config);
		rt.loadModule(module);
		return rt;
	}

	/** Loads a guest and makes it current; earlier builds stay loaded for restores. Returns its build id. */
	async load(module: WebAssembly.Module | BufferSource): Promise<Uint8Array> {
		if (module instanceof WebAssembly.Module) {
			this.loadMs = 0;
			return this.loadModule(module);
		}
		const t = performance.now();
		const compiled = await WebAssembly.compile(module);
		this.loadMs = performance.now() - t;
		return this.loadModule(compiled);
	}

	loadModule(module: WebAssembly.Module): Uint8Array {
		const names = new Set(WebAssembly.Module.exports(module).map((e) => e.name));
		for (const name of REQUIRED_EXPORTS) {
			if (!names.has(name)) {
				throw new MmError("abi", `the module lacks export ${name}: not an mm-wasm v${ABI_VERSION} guest`);
			}
		}
		// The bundle guest: refused here, not at its first import, without both sections.
		const bundled = WebAssembly.Module.imports(module).some(
			(i) => i.module === IMPORT_MODULE && i.name === importName.BUNDLE_READ,
		);
		if (bundled && bundleOf(module).some((s) => s.length === 0)) {
			throw new MmError(
				"abi",
				`the module imports ${IMPORT_MODULE}.${importName.BUNDLE_READ} but lacks its ${bundleSections.join(" or ")} section`,
			);
		}
		const buildId = new Instance({ module, buildId: new Uint8Array(32), key: "" }, this.config).buildId();
		if (buildId.length !== 32) throw new MmError("abi", "mm_build_id is not 32 bytes");
		const key = hex(buildId);
		this.guests = this.guests.filter((g) => g.key !== key);
		this.guests.push({ module, buildId, key });
		return buildId;
	}

	/** Unloads build `id` once its snapshots drained; the current build stays. */
	unload(id: Uint8Array): boolean {
		const key = hex(id);
		if (key === hex(this.currentBuild())) return false;
		const before = this.guests.length;
		this.guests = this.guests.filter((g) => g.key !== key);
		return this.guests.length < before;
	}

	currentBuild(): Uint8Array {
		return this.guests.at(-1)?.buildId ?? new Uint8Array(32);
	}

	builds(): Uint8Array[] {
		return this.guests.map((g) => g.buildId);
	}

	private current(): Guest {
		const g = this.guests.at(-1);
		if (!g) throw new MmError("abi", "no module loaded");
		return g;
	}

	/**
	 * A new session of the current module: in a new instance, or with
	 * {@link RuntimeConfig.sessionsPerInstance} above 1 in an instance of that module
	 * with a free slot.
	 */
	session(options: SessionOptions = sessionOptions()): Session {
		const inst = this.slot(this.current());
		const w = new Writer();
		putOptions(w, options);
		return this.place(inst, w.bytes(), false);
	}

	/**
	 * A session restored from `image` in an instance of the module whose build
	 * wrote it (placed as {@link Runtime.session} places sessions); `unknown-build`
	 * when that build is not loaded. The limits and host-side settings of `options`
	 * apply; the text limit, `lastExprValue` and `hostArgs` come from the image.
	 */
	restore(image: Uint8Array, options: SessionOptions = sessionOptions()): Session {
		const id = imageBuildId(image);
		let guest = this.current();
		if (id) {
			const key = hex(id);
			const found = this.guests.find((g) => g.key === key);
			if (!found) {
				throw new MmError(
					"unknown-build",
					`the snapshot was written by build ${key}, which this runtime has not loaded`,
					id.slice(),
				);
			}
			guest = found;
		}
		const inst = this.slot(guest);
		const w = new Writer(64 + image.length);
		putOptions(w, options);
		w.raw(image);
		return this.place(inst, w.bytes(), true);
	}

	/** Live instances, their sessions and their linear memory in bytes. */
	stats(): { instances: number; sessions: number; memoryBytes: number } {
		let sessions = 0;
		let memoryBytes = 0;
		for (const inst of this.live) {
			sessions += inst.sessions.size;
			memoryBytes += inst.memoryBytes();
		}
		return { instances: this.live.size, sessions, memoryBytes };
	}

	private get perInstance(): number {
		return Math.max(1, Math.floor(this.config.sessionsPerInstance ?? 1));
	}

	/** An instance of `guest` with a free slot, or a new one. */
	private slot(guest: Guest): Instance {
		if (this.perInstance > 1) {
			for (const inst of this.open.get(guest.key) ?? []) {
				if (inst.dead === undefined && inst.sessions.size < this.perInstance) return inst;
			}
		}
		return new Instance(guest, this.config);
	}

	/** Opens a session in `inst` from `input` (options, and the image for a restore). */
	private place(inst: Instance, input: Uint8Array, restore: boolean): Session {
		const s = new Session(inst, this.config, (gone) => this.track(gone.inst));
		try {
			inst.write(input);
			return inst.open(s, () => (restore ? inst.f.mm_session_restore() : inst.f.mm_session_new()) as number, restore);
		} finally {
			this.track(inst);
		}
	}

	/** Keeps the live set and the free slots current after `inst` gained or lost a session. */
	private track(inst: Instance): void {
		const open = this.open.get(inst.guest.key) ?? [];
		const at = open.indexOf(inst);
		const free = this.perInstance > 1 && inst.dead === undefined && inst.sessions.size > 0 && inst.sessions.size < this.perInstance;
		if (free && at < 0) open.push(inst);
		if (!free && at >= 0) open.splice(at, 1);
		if (open.length > 0) this.open.set(inst.guest.key, open);
		else this.open.delete(inst.guest.key);
		if (inst.sessions.size > 0) this.live.add(inst);
		else this.live.delete(inst);
	}
}

const utf8 = new TextDecoder();

/**
 * The bundle guest's prelude and stdlib bytecode: the module's `mm.prelude` and
 * `mm.stdlib` custom sections, read once per module and shared by its instances,
 * served to `mm_host.bundle_read`. Empty for the default guest, which links them in.
 */
const bundles = new WeakMap<WebAssembly.Module, Uint8Array[]>();
function bundleOf(module: WebAssembly.Module): Uint8Array[] {
	let b = bundles.get(module);
	if (!b) {
		b = bundleSections.map((n) => {
			const s = WebAssembly.Module.customSections(module, n)[0];
			return s ? new Uint8Array(s) : new Uint8Array(0);
		});
		bundles.set(module, b);
	}
	return b;
}

/**
 * One wasm instance of a guest and the sessions it holds (one unless
 * {@link RuntimeConfig.sessionsPerInstance} says more). It owns what the
 * sessions share: linear memory, the input and output buffers, the host
 * imports (which name the session they are for) and the trap that ends them
 * all.
 */
class Instance {
	readonly f: Exports;
	readonly guest: Guest;
	private readonly memory: WebAssembly.Memory;
	private bytes: Uint8Array;
	dead: string | undefined;
	/** Live sessions by id. */
	readonly sessions = new Map<number, Session>();
	/** The session `mm_session_new`/`mm_session_restore` is creating, before its id is known. */
	creating: Session | undefined;
	readonly w = new Writer();

	constructor(guest: Guest, config: RuntimeConfig) {
		this.guest = guest;
		const module = guest.module;
		const now = config.now ?? (() => performance.now());
		const wall = config.wallClock ?? (() => Date.now() / 1000);
		const entropy =
			config.entropy ??
			((buf: Uint8Array) => {
				for (let at = 0; at < buf.length; at += 65536) crypto.getRandomValues(buf.subarray(at, at + 65536) as Uint8Array<ArrayBuffer>);
			});
		const base = now();
		const of = (sid: number): Session | undefined => this.sessions.get(sid) ?? this.creating;
		const host = {
			[importName.BUNDLE_READ]: (region: number, offset: number, ptr: number, len: number): number => {
				[region, offset, ptr, len] = [u32(region), u32(offset), u32(ptr), u32(len)];
				const src = bundleOf(module)[region];
				const m = this.mem();
				if (!src || offset + len > src.length || ptr + len > m.length) return 0;
				m.set(src.subarray(offset, offset + len), ptr);
				return 1;
			},
			[importName.NOW_NS]: (sid: number): bigint =>
				BigInt(Math.max(0, Math.round((now() - (of(sid)?.base ?? base)) * 1e6))),
			[importName.PARK]: (sid: number, clear: number): number => {
				const s = of(sid);
				const sig = config.parkSignal;
				const asked = (s?.parked ?? false) || (sig !== undefined && Atomics.load(sig, 0) !== 0);
				if (clear && asked) {
					if (s) s.parked = false;
					if (sig) Atomics.store(sig, 0, 0);
				}
				return asked ? 1 : 0;
			},
			[importName.WALL_CLOCK]: (_sid: number): number => wall(),
			[importName.ENTROPY]: (_sid: number, ptr: number, len: number): void => {
				[ptr, len] = [u32(ptr), u32(len)];
				const m = this.mem();
				if (ptr + len <= m.length) entropy(m.subarray(ptr, ptr + len));
			},
			[importName.OUTPUT]: (sid: number, stream: number, ptr: number, len: number): void => {
				const out = of(sid)?.out;
				if (!out) return;
				[ptr, len] = [u32(ptr), u32(len)];
				const m = this.mem();
				if (ptr + len > m.length) return;
				out(stream === 1 ? "stderr" : "stdout", utf8.decode(m.subarray(ptr, ptr + len)));
			},
			[importName.ZONE]: (sid: number, key: number, keyLen: number, buf: number, cap: number): bigint => {
				const s = of(sid);
				[key, keyLen, buf, cap] = [u32(key), u32(keyLen), u32(buf), u32(cap)];
				const m = this.mem();
				if (!s || key + keyLen > m.length) return -1n;
				let name: string;
				try {
					name = new TextDecoder("utf-8", { fatal: true }).decode(m.subarray(key, key + keyLen));
				} catch {
					return -1n;
				}
				const tzif = s.zone(name);
				if (!tzif) return -1n;
				if (tzif.length <= cap) {
					const m2 = this.mem();
					if (buf + tzif.length > m2.length) return -1n;
					m2.set(tzif, buf);
				}
				return BigInt(tzif.length);
			},
		};
		const imports: WebAssembly.Imports = { [IMPORT_MODULE]: host };
		// A wasm32-wasip1 guest imports std's start-up WASI functions; it reads no files,
		// clock or randomness through them once a session exists.
		const wasi = WebAssembly.Module.imports(module).filter((i) => i.module === "wasi_snapshot_preview1");
		if (wasi.length > 0) imports.wasi_snapshot_preview1 = wasiStubs(() => this.mem(), entropy, wasi);
		let instance: WebAssembly.Instance;
		try {
			instance = new WebAssembly.Instance(module, imports);
		} catch (e) {
			throw new MmError("trap", String(e));
		}
		this.f = instance.exports as unknown as Exports;
		this.memory = this.f.memory;
		this.bytes = new Uint8Array(this.memory.buffer);
		const init = this.f._initialize;
		if (init) this.call(() => init());
		const got = this.call(() => this.f.mm_abi_version() as number);
		if (got !== ABI_VERSION) throw new MmError("abi", `guest speaks mm-wasm ${got}, this host ${ABI_VERSION}`);
	}

	mem(): Uint8Array {
		if (this.bytes.buffer !== this.memory.buffer) this.bytes = new Uint8Array(this.memory.buffer);
		return this.bytes;
	}

	memoryBytes(): number {
		return this.memory.buffer.byteLength;
	}

	/** Runs one export call; any exception from the instance is a trap that ends every session in it. */
	call<T>(f: () => T): T {
		if (this.dead !== undefined) throw new MmError("trap", this.dead);
		try {
			return f();
		} catch (e) {
			if (e instanceof MmError) throw e;
			const message = e instanceof Error ? `${e.name}: ${e.message}` : String(e);
			this.dead = message;
			throw new MmError("trap", message);
		}
	}

	/** Sizes the guest's input buffer and fills it; a bad address is a trap, as a fault in the guest is. */
	write(bytes: Uint8Array): void {
		this.call(() => {
			const at = u32(this.f.mm_input(bytes.length as never) as number);
			const m = this.mem();
			if (at + bytes.length > m.length) throw new RangeError(`mm_input returned ${at}, out of bounds for ${bytes.length} bytes`);
			m.set(bytes, at);
		});
	}

	/** The output buffer as a reader over linear memory (valid until the next call). */
	output(): Reader {
		const packed = unsigned(this.call(() => this.f.mm_output() as bigint));
		const at = Number(packed >> 32n);
		const len = Number(packed & 0xffffffffn);
		const m = this.mem();
		if (at + len > m.length) throw new MmError("abi", "output out of bounds");
		return new Reader(m.subarray(at, at + len));
	}

	decode<T>(f: (r: Reader) => T): T {
		try {
			return f(this.output());
		} catch (e) {
			if (e instanceof Bad) throw new MmError("abi", e.message);
			throw e;
		}
	}

	buildId(): Uint8Array {
		this.call(() => this.f.mm_build_id());
		return this.output().take(32).slice();
	}

	/**
	 * Creates a session with `create` (`mm_session_new` or `mm_session_restore`, after
	 * the input was written) and registers it; a failure leaves the instance as it was.
	 */
	open(session: Session, create: () => number, restore: boolean): Session {
		this.creating = session;
		let id: number;
		try {
			id = this.call(create);
		} finally {
			this.creating = undefined;
		}
		if (id >= 0) {
			session.sid = id;
			this.sessions.set(id, session);
			return session;
		}
		if (id === -1 && !restore) {
			const e = this.decode(getException);
			throw new MmError("py", `${e.typeName}: ${e.message}`, e);
		}
		if (id === -1) {
			const e = this.decode(getSnapshotError);
			throw new MmError("snapshot", snapshotMessage(e), e);
		}
		throw new MmError("abi", this.decode((r) => r.string()));
	}
}

/**
 * One micro-monty session. By default it has a wasm instance of its own; with
 * {@link RuntimeConfig.sessionsPerInstance} above 1 it may share one, and with
 * it the instance's memory and its traps.
 */
export class Session {
	/** @internal */ readonly inst: Instance;
	/** @internal */ sid = 0;
	/** @internal */ parked = false;
	/** @internal */ readonly base: number;
	/** @internal */ out: Output | undefined;
	private disposed = false;
	private readonly config: RuntimeConfig;
	private readonly release: (s: Session) => void;
	private zones: ZoneSource | undefined;
	private zoneCache: [string, Uint8Array | undefined] | undefined;

	/** @internal Use {@link Runtime.session} or {@link Runtime.restore}. */
	constructor(inst: Instance, config: RuntimeConfig, release: (s: Session) => void) {
		this.inst = inst;
		this.config = config;
		this.release = release;
		this.base = (config.now ?? (() => performance.now()))();
	}

	/** @internal The zone source's answer for `key`, cached for the last key. */
	zone(key: string): Uint8Array | undefined {
		if (this.zoneCache && this.zoneCache[0] === key) return this.zoneCache[1];
		const tzif = this.zones?.(key);
		this.zoneCache = [key, tzif];
		return tzif;
	}

	/** @internal Runs one export call of this session. */
	call<T>(f: () => T): T {
		if (this.disposed) throw new MmError("abi", "the session was disposed");
		return this.inst.call(f);
	}

	private failure(code: number): MmError {
		const r = this.inst.output();
		try {
			switch (code) {
				case status.PY_ERROR: {
					const e = getException(r);
					return new MmError("py", `${e.typeName}: ${e.message}`, e);
				}
				case status.SNAPSHOT_ERROR: {
					const e = getSnapshotError(r);
					return new MmError("snapshot", snapshotMessage(e), e);
				}
				case status.DEFINE_ERROR: {
					const e = getDefineError(r);
					return new MmError("define", defineMessage(e), e);
				}
				default:
					return new MmError("abi", r.string());
			}
		} catch (e) {
			return new MmError("abi", e instanceof Error ? e.message : String(e));
		}
	}

	private progress(code: number): Progress {
		return this.inst.decode((r) => {
			switch (code) {
				case event.COMPILE_ERROR: {
					const e = getCompileError(r);
					throw new MmError("compile", e.rendered, e);
				}
				case event.RESUME_ERROR: {
					const e = getResumeError(r);
					throw new MmError("resume", resumeMessage(e), e);
				}
				case event.BAD_REQUEST:
					throw new MmError("abi", r.string());
				default:
					return getProgress(code, r);
			}
		});
	}

	private input(fill: (w: Writer) => void): void {
		if (this.disposed) throw new MmError("abi", "the session was disposed");
		fill(this.inst.w.reset());
		this.inst.write(this.inst.w.bytes());
	}

	private get f(): Exports {
		return this.inst.f;
	}

	/** Whether a trap ended this session (and every other session of its instance). */
	get isDead(): boolean {
		return this.inst.dead !== undefined;
	}

	/** The guest's snapshot build id. */
	buildId(): Uint8Array {
		return this.inst.buildId();
	}

	/** Where `print` and `sys.stdout`/`stderr` writes go. Host-side: set it again after a restore. */
	setOutput(out: Output | undefined): void {
		this.out = out;
	}

	/** `Session::define`. */
	define(module: HostModule): HostFunctionRef[] {
		this.input((w) => putModule(w, module));
		const code = this.call(() => this.f.mm_define(this.sid as never) as number);
		if (code !== status.OK) throw this.failure(code);
		return this.inst.decode(getFunctionRefs);
	}

	/** Compiles `source` in the guest and starts it. */
	start(source: string, filename = "cell.py"): Progress {
		this.input((w) => {
			w.str(filename);
			w.str(source);
		});
		return this.progress(this.call(() => this.f.mm_start(this.sid as never) as number));
	}

	/** `Session::resume`. */
	resume(resume: Resume): Progress {
		this.input((w) => putResume(w, resume));
		return this.progress(this.call(() => this.f.mm_resume(this.sid as never) as number));
	}

	/** `Session::cancel`: whether a cell was stopped. */
	cancel(): boolean {
		return this.call(() => this.f.mm_cancel(this.sid as never) as number) !== 0;
	}

	/** `Session::reset`. */
	reset(): void {
		const code = this.call(() => this.f.mm_reset(this.sid as never) as number);
		if (code !== status.OK) throw this.failure(code);
	}

	/**
	 * `Session::snapshot` (`maxBytes` omitted: capped at the memory limit). A copy of the image.
	 * It cannot park: it collects in full first, about 1.4 ms per MiB of image [M], so no host
	 * timeout should cover it (README, "Differences from the wasmtime host").
	 */
	snapshot(maxBytes?: number): Uint8Array {
		const code = this.call(() => this.f.mm_snapshot(this.sid as never, u64(maxBytes) as never) as number);
		if (code !== status.OK) throw this.failure(code);
		const r = this.inst.output();
		return r.data.slice();
	}

	/** `Session::set_fuel`; `null` is unlimited. */
	setFuel(fuel: number | bigint | null): void {
		this.call(() => this.f.mm_set_fuel(this.sid as never, u64(fuel) as never));
	}

	/** `Session::fuel`; `null` is unlimited. */
	fuel(): number | null {
		return optNumber(this.call(() => this.f.mm_fuel(this.sid as never) as bigint));
	}

	/**
	 * `Session::fuel_wanted`: after `parked` for fuel, what the next native operation
	 * will charge when that is more than was left (0 after an ordinary fuel park).
	 * Check it before granting another slice.
	 */
	fuelWanted(): number {
		return Number(unsigned(this.call(() => this.f.mm_fuel_wanted(this.sid as never) as bigint)));
	}

	/** `Session::set_fuel_total`; `null` is no cap. */
	setFuelTotal(total: number | bigint | null): void {
		this.call(() => this.f.mm_set_fuel_total(this.sid as never, u64(total) as never));
	}

	/** `Session::fuel_total`; `null` is no cap. */
	fuelTotal(): number | null {
		return optNumber(this.call(() => this.f.mm_fuel_total(this.sid as never) as bigint));
	}

	/**
	 * `Session::pending`: the stop the session is at, as it was reported (a restored
	 * session's too), or `null` between cells.
	 */
	pending(): Progress | null {
		const p = this.progress(this.call(() => this.f.mm_pending(this.sid as never) as number));
		return p.kind === "complete" && p.completion === null ? null : p;
	}

	/** `Session::phase`. */
	phase(): Phase {
		const code = this.call(() => this.f.mm_phase(this.sid as never) as number);
		try {
			return phaseFromCode(code);
		} catch (e) {
			throw new MmError("abi", (e as Error).message);
		}
	}

	/** `Session::set_hint`: `group` is a `HintGroup::key` (`network`, `data`, ...). */
	setHint(group: string, text: string | null): void {
		this.input((w) => {
			w.str(group);
			w.optStr(text);
		});
		const code = this.call(() => this.f.mm_set_hint(this.sid as never) as number);
		if (code !== status.OK) throw this.failure(code);
	}

	/** `Session::set_text_cut`. */
	setTextCut(cut: TextCut): void {
		this.call(() => this.f.mm_set_text_cut(this.sid as never, textCutCode(cut) as never));
	}

	/** `Session::set_local_timezone` (session state: snapshots carry it). */
	setLocalTimezone(tzif: Uint8Array | null): void {
		this.input((w) => w.optBytes(tzif));
		this.call(() => this.f.mm_set_local_timezone(this.sid as never));
	}

	/** `Session::set_zone_source`. Host-side: set it again after a restore; answers only with `zoneSource` on. */
	setZoneSource(source: ZoneSource | undefined): void {
		this.zones = source;
		this.zoneCache = undefined;
	}

	/** `Session::call_log`. */
	callLog(): HostCallRecord[] {
		this.call(() => this.f.mm_call_log(this.sid as never));
		return this.inst.decode(getCallLog);
	}

	/** `Session::environment`: this session's limits and fuel total. */
	environment(): string {
		this.call(() => this.f.mm_environment(this.sid as never));
		return this.inst.decode((r) => r.string());
	}

	/** `Session::stubs`: (module, stub text) pairs. */
	stubs(): [string, string][] {
		this.call(() => this.f.mm_stubs(this.sid as never));
		return this.inst.decode(getStubs);
	}

	/** `Session::host_modules`. */
	hostModules(): HostModule[] {
		this.call(() => this.f.mm_host_modules(this.sid as never));
		return this.inst.decode(getModules);
	}

	/** `Session::set_module_dunders`. */
	setModuleDunders(name: string | null, file: string | null): void {
		this.input((w) => {
			w.optStr(name);
			w.optStr(file);
		});
		const code = this.call(() => this.f.mm_set_module_dunders(this.sid as never) as number);
		if (code !== status.OK) throw this.failure(code);
	}

	/** `Session::set_int_bits_text`. */
	setIntBitsText(on: boolean): void {
		this.call(() => this.f.mm_set_int_bits_text(this.sid as never, (on ? 1 : 0) as never));
	}

	/** `Session::set_gc_stress` (engine testing). */
	setGcStress(on: boolean): void {
		this.call(() => this.f.mm_set_gc_stress(this.sid as never, (on ? 1 : 0) as never));
	}

	/**
	 * `Session::set_gc_yield`: under a fuel limit, a slice also ends after a garbage
	 * collection step, as a `fuel` park with fuel left. No fuel is charged for it. Returns
	 * whether the guest has the optional export (a guest that predates it does not).
	 */
	setGcYield(on: boolean): boolean {
		const f = this.f.mm_set_gc_yield;
		if (!f) return false;
		this.call(() => f(this.sid as never, (on ? 1 : 0) as never));
		return true;
	}

	/**
	 * Parks with `deadline` at the first safepoint `afterMs` from now (`null` clears
	 * it). One-shot. On Workers the clock stands still during synchronous execution.
	 */
	setDeadline(afterMs: number | null): void {
		const now = this.config.now ?? (() => performance.now());
		const at = afterMs === null ? U64_MAX : BigInt(Math.round((now() - this.base + afterMs) * 1e6));
		this.call(() => this.f.mm_set_deadline(this.sid as never, at as never));
	}

	/**
	 * Requests a park (`requested`): the running cell parks at its next safepoint check,
	 * or the next cell at its first. JavaScript is single-threaded, so this lands from a
	 * callback the guest makes (output, zone, clock) or between calls; another thread
	 * uses {@link RuntimeConfig.parkSignal}.
	 */
	requestPark(): void {
		this.parked = true;
	}

	/** The linear memory of this session's instance in bytes, shared by every session in it. */
	memoryBytes(): number {
		return this.inst.memoryBytes();
	}

	/** Fault injection, when the guest was built with `test-hooks`. */
	testPanic(op: number): void {
		const f = this.f.mm_test_panic;
		if (!f) throw new MmError("abi", "guest built without test-hooks");
		this.call(() => f(op as never));
	}

	/**
	 * Drops the session and its heap. Its instance is released when no session uses it
	 * any more (and nothing else references it); the instance's linear memory does not
	 * shrink while it lives.
	 */
	dispose(): void {
		if (this.disposed) return;
		if (this.inst.dead === undefined) {
			try {
				this.inst.f.mm_session_drop(this.sid as never);
			} catch {
				// The instance is unusable anyway.
			}
		}
		this.disposed = true;
		this.out = undefined;
		this.zones = undefined;
		this.inst.sessions.delete(this.sid);
		this.release(this);
	}
}

function snapshotMessage(e: SnapshotError): string {
	switch (e.kind) {
		case "incompatible":
			return `the snapshot has format ${e.format} and build ${hex(e.buildId)}`;
		case "corrupt":
			return `corrupt snapshot: ${e.what}`;
		case "internal":
			return `snapshot failed: ${e.message}`;
		default:
			return `snapshot: ${e.kind}`;
	}
}

function defineMessage(e: DefineError): string {
	switch (e.kind) {
		case "invalid":
		case "poisoned":
			return `${e.kind}: ${e.text}`;
		case "raised":
			return `${e.error.typeName}: ${e.error.message}`;
		default:
			return "the session is busy";
	}
}

function resumeMessage(e: ResumeError): string {
	return e.kind === "unexpected" ? `nothing to resume this way in phase ${e.phase}` : `${e.kind}: call ${e.call}`;
}

function wasiStubs(
	mem: () => Uint8Array,
	entropy: (b: Uint8Array) => void,
	wanted: WebAssembly.ModuleImportDescriptor[],
): Record<string, Fn> {
	const view = () => new DataView(mem().buffer);
	// Pointers and lengths are u32 (see `u32` above); DataView's own reads are unsigned.
	const known: Record<string, Fn> = {
		environ_sizes_get: ((count: number, size: number) => {
			view().setUint32(u32(count), 0, true);
			view().setUint32(u32(size), 0, true);
			return 0;
		}) as Fn,
		args_sizes_get: ((count: number, size: number) => {
			view().setUint32(u32(count), 0, true);
			view().setUint32(u32(size), 0, true);
			return 0;
		}) as Fn,
		environ_get: (() => 0) as Fn,
		args_get: (() => 0) as Fn,
		fd_write: ((_fd: number, iovs: number, n: number, written: number) => {
			const v = view();
			let total = 0;
			for (let i = 0; i < u32(n); i++) total += v.getUint32(u32(iovs) + i * 8 + 4, true);
			v.setUint32(u32(written), u32(total), true);
			return 0;
		}) as Fn,
		clock_time_get: ((_id: number, _precision: bigint, at: number) => {
			view().setBigUint64(u32(at), BigInt(Date.now()) * 1_000_000n, true);
			return 0;
		}) as Fn,
		random_get: ((ptr: number, len: number) => {
			[ptr, len] = [u32(ptr), u32(len)];
			entropy(mem().subarray(ptr, ptr + len));
			return 0;
		}) as Fn,
		proc_exit: ((code: number) => {
			throw new Error(`proc_exit(${code})`);
		}) as Fn,
	};
	const out: Record<string, Fn> = {};
	// ENOSYS for the rest.
	for (const i of wanted) out[i.name] = known[i.name] ?? ((() => 52) as Fn);
	return out;
}
