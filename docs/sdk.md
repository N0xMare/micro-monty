# The micro-monty Rust SDK

`micro-monty` is a Python 3.14 subset that runs inside your Rust process.
It has no ambient I/O: files, network, clocks and randomness reach Python only
through functions you provide. A running cell can stop at any safepoint, at
any call depth, and continue later. That can be in the same process, on
another thread, or from a snapshot in another process or on another target.

This guide is the `micro-monty` crate's documentation, and its examples
are its doctests. The design and its measurements are in the
[spec](https://github.com/N0xMare/micro-monty/blob/main/docs/spec.md);
numbers here are labelled \[M\] measured, \[S\] sourced or \[E\] estimated.

```toml
[dependencies]
micro-monty = "0.0.1"   # or { git = "https://github.com/N0xMare/micro-monty" }
```

Releases are `0.0.x` for now, and Cargo treats each one as incompatible
with the next: `"0.0.1"` never upgrades to 0.0.2 on its own. Moving to a
new release is a deliberate version change, and its snapshots follow
[Upgrading: drain old builds](#upgrading-drain-old-builds).

It is pure Rust: no C toolchain and no files beside it. The bundled stdlib
is compiled into the crate. It builds for `wasm32-wasip1` and
`wasm32-unknown-unknown` too.

## A first cell

```rust
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use micro_monty::{
    Engine, EngineConfig, HostError, HostErrorKind, HostModule, HostValue, Limits, ParkReason,
    Progress, Resume, Session,
};

let limits = Limits::default().with_memory(64 << 20).with_fuel(Some(1_000_000));
let engine = Engine::new(EngineConfig::default().with_limits(limits));
let mut session = Session::new(&engine)?;
session.set_output(Box::new(|text| print!("{text}")));
# let printed = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
# let sink = std::sync::Arc::clone(&printed);
# session.set_output(Box::new(move |text| sink.lock().unwrap().push_str(text)));
session.define(&HostModule::untyped("", &["lookup"]))?;
session.define(&HostModule {
    constants: vec![("ROOT".into(), HostValue::Str("/work".into()))],
    ..HostModule::untyped("fs", &["read"])
})?;

let code = engine.compile(
    "import fs\nprice = lookup('apple') * 3\nprint(fs.ROOT, fs.read('a.txt'))\nprice",
    "cell.py",
)?;
let mut progress = session.start(&code);
# let mut done = None;
loop {
    progress = match progress {
        Progress::HostCall { name, args, .. } => {
            let answer = match (name.as_str(), args.as_slice()) {
                ("lookup", [HostValue::Str(item)]) if item == "apple" => {
                    Resume::Value(HostValue::Int(2))
                }
                ("fs.read", [HostValue::Str(path)]) => {
                    Resume::Value(HostValue::Str(format!("<{path}>")))
                }
                _ => Resume::Error(HostError::new(
                    HostErrorKind::ValueError,
                    format!("unknown call {name}"),
                )),
            };
            session.resume(answer)?
        }
        Progress::Parked { reason: ParkReason::Fuel } => {
            session.set_fuel(Some(1_000_000));
            session.resume(Resume::Continue)?
        }
        Progress::Complete(value) => {
            let repr = value.map(|c| c.repr);
            println!("=> {repr:?}");
#           done = repr;
            break;
        }
        Progress::Error(e) => break eprint!("{}", e.traceback),
        _ => unreachable!("no futures in this cell"),
    }
}
// prints "/work <a.txt>", then `=> Some("6")`
# assert_eq!(*printed.lock().unwrap(), "/work <a.txt>\n");
# assert_eq!(done.as_deref(), Some("6"));
# Ok(()) }
```

The engine runs on the caller's thread until it reaches one of the events in
`Progress`. There are no callbacks into your code except output, clocks and
entropy, so the driving loop above is the whole integration.

## Engine, code and sessions

| Type | Role | Threads |
|---|---|---|
| `Engine` | Holds the `EngineConfig` (limits) and the cache of decoded bundled modules its sessions share. Clones share the cache, and so does `engine.with_limits(limits)`, an engine for sessions with other limits. Compiles and restores. | `Send + Sync` |
| `Code` | `Engine::compile(source, filename)` output, or `Session::compile`'s (the same, charged to that session's memory limit): parsed and compiled once, reusable in any session. `warnings()` lists `SyntaxWarning`s. | `Send + Sync` |
| `Session` | One Python interpreter: heap, globals, imported modules, the running cell. | `Send`, not `Sync` |

- **Cells.** `Session::start(&code)` runs one cell in the session's globals.
  Later cells see what earlier ones defined, as in a notebook. A cell whose
  last statement is an expression completes with `Completion { repr, str,
  type_name, value }`, where `value` is the result as a `HostValue` when it
  converts. `set_last_expr_value(false)` turns that off for scripts, so no
  user `__repr__` runs. `start` cancels a cell that is still stopped.
- **Reset.** `Session::reset()` clears globals and imported modules. Host
  functions and host modules stay. `set_module_dunders` sets `__name__` and
  `__file__` (a new session runs as `__main__`).
- **Errors.** `CompileError` (a `SyntaxError`, including "micro-monty does not
  support X", or a `RecursionError` for nesting too deep) carries the line,
  column and a CPython-format `rendered` report. An uncaught exception ends
  the cell with `Progress::Error(PyException { type_name, message,
  traceback, truncated })`, with the traceback in CPython 3.14's format
  (see "Long results" for when it is cut). The host's
  own mistakes have their own types: `DefineError` (a refused host module),
  `ResumeError` (an answer that does not fit the stop, with the session's
  `Phase`) and `SnapshotError`. Every error type the API returns
  implements `std::error::Error`, and they and the result types are
  `#[non_exhaustive]`: match them with a wildcard arm. Build the plain
  structs a host constructs (`HostParam`, `HostClass`, `HostException`,
  ...) with `..Default::default()`, `HostError` with `..HostError::new(..)`,
  or the constructors `HostError::new`,
  `HostSignature::any` and `HostModule::untyped`.
- **Threads.** A session can be stopped on one thread and resumed on another.
  Every callback it holds must therefore be `Send`. On tokio, move the session
  into `spawn_blocking` for each step, or give it fuel in slices so each step
  returns quickly. Share one `Engine` and its `Code` across tasks with `Arc`.
  `tests/send.rs` checks these bounds at compile time.
- **Many sessions.** Sessions share no mutable state, so one process can
  run many sessions with independent limits. A new session binds the
  prelude's natives and loads stdlib units on first use; `mm run print(0)`
  costs 0.90M instructions for the whole process, 0.57M from glibc's
  `__libc_start_main` on (`bench/callgrind.py`, eb78933) \[M\]. Sessions created or restored
  from one `Engine` (or its clones) share each bundled module's decoded
  bytecode: the first import of a module in the process decodes it, and
  later imports and restores reuse it (see "The module cache" under
  [Embedding guarantees](#embedding-guarantees)). Keep one `Engine` for
  the process.
- **Panics.** The engine must not panic on any input. Built with unwinding,
  the facade catches an engine panic and poisons only that session: every
  later call returns a `SystemError` and `is_poisoned()` is true. With
  `panic = "abort"` a panic ends the process, so in-process embedders should
  build with unwinding.

## Limits

| Resource | Setting | When it runs out |
|---|---|---|
| Heap | `Limits::memory` (default 256 MiB): arena, retained code and sources, caches, frames, task stacks, host-module definitions | catchable `MemoryError`; big allocations (`*`, `pow`, `<<`, `join`, ...) are checked before they start |
| CPU | `Limits::fuel` per cell (default unlimited); `Session::set_fuel` for the next cell or a parked one; `Session::set_fuel_total` for all of a session's cells together. One unit per call or backward jump, plus proportional charges by long natives | parks: `Parked { reason: Fuel }` |
| Wall time | `Session::set_deadline(Instant)`, or `set_deadline_with(now, deadline)` with your own clock | parks: `Parked { reason: Deadline }` (one-shot; set it again before resuming) |
| Python stack | `Limits::recursion_depth` (default 1000) | `RecursionError` |
| Rust stack | running code keeps nesting in the arena; the parser and compiler are depth-counted (see [Embedding guarantees](#embedding-guarantees)) | `RecursionError`, never a stack overflow |
| Source | `MAX_SOURCE_BYTES` = 256 KiB; nesting and per-function limits in [Code size limits](#code-size-limits) | `CompileError` |
| Host arguments | `Session::set_host_args(HostArgs { max_bytes, max_depth, lenient })` (default 16 MiB, depth 100, strict) | an exception in Python before the host sees the call (`ValueError` past `max_bytes`, `RecursionError` past `max_depth`, `TypeError` for a value with no host form or a non-`str` dict key, `MemoryError` past the memory limit), or `HostValue::Unconverted` pieces when `lenient` |
| Completion text | `Session::set_text_limit(bytes)` (default 1 MiB, or the memory limit if less), `set_text_cut(TextCut)`; printed output is not cut, the host's callback receives all of it | `repr`/`str` cut at a character boundary (`Head`), or head and tail with a marker, containers keeping their shape (`HeadTail`); `*_truncated` set; `value` stays whole |
| int ↔ str | CPython's 4300-digit limit | `ValueError`; `set_int_bits_text(true)` renders `<int with N bits>` in completions instead |
| One object | 64 MiB of a `str`'s or `bytes`' data, or of a list's, tuple's, dict's or set's slots, whatever `Limits::memory` is (`mm_core::MAX_OBJECT_BYTES`): a list holds at most 8,388,608 items, a dict 2,097,152 entries and a set 5,033,164 items \[M\] | catchable `MemoryError` |
| `json.dumps` result | min(0.48 × `Limits::memory`, 64 MiB) of text \[M\]: the text is one contiguous block, and the old and new buffers coexist while it grows, so growing needs about twice the text. At a 16 MiB limit (a Workers session), that is about 7.7 MiB | catchable `MemoryError` |
| Starred displays, calls and f-strings | the largest result that completes beside inputs as large as itself, as a fraction of `Limits::memory` (and at most 64 MiB) \[M\]: `[*a, *b]` 0.35, `(*a, *b)`, `x, *y = [*a, *b]` and `f(*a, *b)` into a function's `*args` 0.25 (the display's list grows once per starred item, and its old buffer stays until the next collection; a tuple or `*args` is copied from the list), an f-string 0.30 (joined in a native buffer, then copied into its `str`) | catchable `MemoryError` |
| Int size and int work | 2^26 bits per int; a superlinear int operation (`*`, `//`, `%`, `divmod`, `**`, `pow(a, e, m)`, `math.factorial`/`comb`/`perm`/`isqrt`/`gcd`/`lcm`, ...) estimated to take more than about 45 ms | `MemoryError` before it starts (`integer operation too large` for the work cap), whatever the fuel |
| `ast.parse` / `tokenize` source | 32 KiB / 64 KiB | `MemoryError` |
| Snapshot output | `snapshot_with_limit(max_bytes)` (default: the memory limit) | `SnapshotError::TooLarge` |

Every byte count the API takes or returns is a `u64`, on every target:
`Limits::memory`, `MAX_SOURCE_BYTES`, `HostArgs::max_bytes`,
`set_text_limit`, `snapshot_with_limit`, `head_tail`, the module cache,
mm-cell-worker's `Config` caps and the mm-wasm exports. The engine clamps
to the platform's address space only where it allocates, so a limit
larger than that (`u64::MAX` included) means no limit, never a panic. (The
mm-wasm exports keep their own `u64::MAX` sentinels, such as "the default
cap" for `mm_snapshot`.)

`Limits::memory` is a bound on what the session retains, not on process RSS;
[Embedding guarantees](#embedding-guarantees) lists what else it covers and
what it does not. Fuel bounds work; bounded native steps bound latency. A
native operation charges fuel in proportion to its work, and one whose
work a guest can make large runs in steps of at most 2^17 units, parking
between them, or is one operation over one object (docs/spec.md §5.1;
every row of `tests/steps.rs` is `Bounded`). Collecting an
iterator that need not end
(`list(enumerate(it))`, `sorted(gen)`, `dict(zip(it, it))`, `*args`) parks between chunks of
65,536 items. The large int operators, the big-int builtins, `sorted` and
`list.sort` of a long list, and `math.factorial`/`perm`/`comb` park before
starting when the fuel left cannot pay their estimate, and a long sort
also parks between slices of its merge. Straight-line code checks at
least every 64 instructions that can do long work. A long container
`repr` goes on in Python after at most 2^17 visits. No object holds more
than 64 MiB of data or slots, whatever the memory limit, so one step over
one object is bounded too: allocating a fresh 64 MiB block is one step of
about 30–40 ms natively \[M\] (docs/spec.md §5.1).

**Fuel is per cell.** Each `start` gives the cell `Limits::fuel`, whatever
the previous cell left, unless `set_fuel` was called since the last cell,
in which case the next cell gets that instead (once). On a parked cell,
`set_fuel` refuels it before `Continue`. A session total caps everything
on top: with `set_fuel_total(Some(n))`, each cell starts with the smaller
of its budget and what is left of `n`, and the fuel every cell charges
while its fuel is limited, refuelled or not, is taken from it (a parked
cell refuelled with `set_fuel(None)` runs on without being counted). Once it is spent, every cell parks on
fuel at its first safepoint (a call, a backward jump or the check the
compiler puts in long straight-line code) or at an int operation or sort
that parks before starting; a short cell with none of these still runs
to its end, and `fuel_total()` is `Some(0)`; refuelling a parked cell is the
host's decision and is not capped. Snapshots and templates keep what is
left of the total, and a stopped cell keeps the fuel it had left; the
per-cell budget is the restoring engine's `Limits::fuel`.

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default());
# let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?;
# let budget = 1_000_000_u64;
# let _ = (engine, code, budget);
# let _ = {
session.set_fuel_total(Some(50 * budget));   // every cell of the session together
if let Progress::Parked { reason: ParkReason::Fuel } = session.start(code) {
    if session.fuel_total() == Some(0) {
        session.cancel();                     // the session's budget is spent
    }
}
# }; Ok(()) }
```

`Engine::environment()` describes the engine's limits for a model's
prompt; `Session::environment()` the session's own (a restored session's
memory limit may be smaller) and what is left of its fuel total.


## Output

`set_output(Box<dyn FnMut(&str) + Send>)` receives `print`, `sys.stdout` and
`sys.stderr` writes merged, in production order. `set_output_streams` does
the same with each piece tagged `Stream::Stdout` or `Stream::Stderr`. Output
is delivered synchronously as it is produced, so the host enforces its own
caps; a code-mode worker typically caps a cell's output at about 1 MiB.

## Host functions and host modules

The host surface is plain data: a `HostModule` per module, defined with
`Session::define`.

```rust ignore
pub struct HostModule {
    pub name: String,                          // "" = builtins; "fs.meta" = a submodule
    pub functions: Vec<HostSignature>,         // typed, or HostSignature::any(name)
    pub classes: Vec<HostClass>,
    pub constants: Vec<(String, HostValue)>,
    pub exceptions: Vec<HostException>,       // ToolError(RuntimeError) and the like
}
```

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default());
# let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?;
# let budget = 1_000_000_u64;
# let _ = (engine, code, budget);
# let _ = {
let ids = session.define(&HostModule::untyped("", &["lookup"]))?;  // builtins; globals may shadow them
session.define(&HostModule {
    constants: vec![("ROOT".into(), HostValue::Str("/".into()))],
    ..HostModule::untyped("fs", &["read", "write"])
})?;
session.define(&HostModule::untyped("fs.meta", &["stat"]))?;      // also the attribute `fs.meta`
# }; Ok(()) }
```

- **Whole-module replacement.** Each `define` replaces everything under that
  name, so the order of definitions does not matter, and nothing from an
  earlier definition lingers. `HostModule::untyped(name, &[])` empties a
  module.
- **Untyped functions** are `HostSignature::any(name)`, the signature
  `(*args: Any, **kwargs: Any) -> Any`. Calls report the arguments as given.
  Typed functions and classes are below.
- **Between cells only.** `define` refuses while a cell is stopped
  (`DefineError::Busy`). It validates the whole module first (names,
  parameter lists, duplicates, values nested deeper than 1000 containers:
  `DefineError::Invalid` with the reason), so a refused definition changes
  nothing. `DefineError::Raised` carries a `MemoryError` when the definition
  does not fit the memory limit.
- **Reserved names.** `import` finds micro-monty's own modules first, so a
  module named like one (`json`, `json.decoder`, `math`, `collections`,
  `builtins`, any bundled module, enabled in this build or not) is refused
  with `DefineError::Invalid`. Names under a bundled package that are not
  its modules (`json.tools`) are the host's.
- **Size.** A module may have any number of functions, classes, methods and
  exceptions. The Python that typed functions and classes are made of is
  generated per definition and compiled **once per `Engine`**: the first
  session to define a module compiles it (about 0.05 ms per typed
  function natively), and every later session of that Engine and its
  clones that defines the same module reuses the code (see "The module
  cache" under [Embedding guarantees](#embedding-guarantees)). A session
  still builds its own function objects: `Session::new` plus `define`
  of 100 typed functions takes about 0.9 ms warm, against 6 ms cold \[M\].
- **Persistence.** Definitions survive `reset` and are part of snapshots and
  templates. `session.host_modules()` returns them as given, for example to
  check what a restored session has. An image carries each module's
  definition, but refers to its compiled code by a key (a hash of the
  generated source) instead of storing it; a restore takes the code from
  the restoring Engine's cache when the image's definition is the one the
  cached code was built from (the cache keeps a hash of the definition
  beside the code; hashing each module's definition costs a warm restore
  of 100 typed functions about 6 µs, 1.5% \[M\]), else generates it again
  from the definition in the image (and compiles it, unless the cache
  holds it under the key), and refuses the image
  (`SnapshotError::Corrupt`) if that does not give the code under the
  image's key. So a restored session never runs other host code than the
  one it was saved with, and the definition it reports is the one that
  code was generated from. Code the Engine does not share is stored in the
  image as before: past the cache's limit, with the cache off, for an
  earlier definition a cell still holds, and on an Engine made with
  `with_host_code_in_snapshots(true)`.
  Keyed images are the default. Hosts that restore every image on a new
  Engine (one session per process or wasm instance) should turn on
  `with_host_code_in_snapshots(true)`: there, a keyed image compiles its
  code again, which costs what the `define` did (100 typed functions:
  6.6 ms, against 0.87 ms for stored code that only decodes \[M\]). The
  mm-wasm guest stores its code, and so does the `mm-cell-worker` binary
  (`NativeEngine::with_host_code_in_snapshots(true)`; its protocol carries
  only untyped tool names and it takes no snapshots, so nothing changes
  there yet).

A call from any frame (a loop, a comprehension, a sort key, a dunder, a
generator) converts its arguments to `HostValue`s and stops the cell with:

```rust ignore
Progress::HostCall { call: u64, function: u32, name: String, args: Vec<HostValue>, kwargs: Vec<(String, HostValue)> }
```

`name` is the call name: `function` for builtins, `module.function`, or
`module.Class.method`. `function` is the function's id, so a host can
dispatch on a `u32` instead of comparing strings:

- **Assignment.** `define` returns a `HostFunctionRef { name, id }` per
  function, functions first, then each class's methods.
- **Stability.** An id is stable per call name for the session's lifetime.
  Redefining a module keeps its functions' ids, and new names get new ones.
- **Persistence.** Snapshots and templates carry the ids, so a restored
  session reports the same ones.
- **Lookup.** `session.host_function_id(name)` and
  `host_function_name(id)` map one way and the other.

`call` increases per session. Answer with one of:

- `Resume::Value(v)`: the call returns `v` in place. Calls are synchronous by
  default: models write `text = fs.read(p)`, and a forgotten `await` cannot
  happen.
- `Resume::Error(HostError::new(kind, message))`: the call raises
  `kind(message)` in place. `HostErrorKind` covers 18 built-in classes, from
  `ValueError` to `FileNotFoundError`; `from_name` maps a class name. A
  host error can also carry a payload and raise a host-named class; see
  [Host errors](#host-errors).
- `Resume::Future`: the call returns an awaitable. When every task is blocked
  on futures the cell stops with `Progress::Waiting { calls }`. Deliver any
  subset of results with `Resume::Batch(vec![(call, Ok(v)), (call, Err(e))])`;
  the cell waits again for the rest. `asyncio.gather` over host calls this way
  yields one batch for the host to run concurrently.

Whether a host function is sync, async or both is your API decision; the
engine supports both for every function. A refused answer
(`ResumeError::{Unexpected, UnknownCall, AlreadySettled}`) changes nothing.

**Return structured values, not JSON strings.** A tool whose result is a
`HostValue::Dict` or `List` saves the model a `json.loads` and a class of
parse errors. Anthropic's convention returns strings; in the codemode
census, 311 of the 396 solutions written against string results parsed
them with `json.loads` first \[M\] (`conformance/codemode`). Raw text (a file, a log) stays a
`Str`.

**Timeouts.** Time a host call out on the host side and answer
`Resume::Error(HostError::new(HostErrorKind::TimeoutError, "fetch timed out
after 30 s"))` (or the same in a `Resume::Batch`), so the cell sees
`TimeoutError`, the class models already catch, as in Anthropic's
`TimeoutError: Calling tool [...] timed out` \[S\]. For a whole cell, set a
deadline (`set_deadline`); it parks rather than raising, so the host
decides: resume with a new deadline, or `cancel()` and report "the cell
timed out" (a host that wants the cell to see an exception reports
`TimeoutError` in its own result). The environment summary tells the model
this convention.

### Host errors

`HostError` has two optional fields beside `kind` and `message`:

```rust ignore
pub struct HostError {
    pub kind: HostErrorKind,
    pub message: String,
    pub data: Option<HostValue>,   // the exception's `data` attribute
    pub class: Option<String>,     // a host exception class, replaces `kind`
}
```

A host exception class is declared with the module, as a subclass of one of
the `HostErrorKind` classes:

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default());
# let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?;
# let budget = 1_000_000_u64;
# let _ = (engine, code, budget);
# let _ = {
session.define(&HostModule {
    exceptions: vec![HostException { name: "ToolError".into(), base: HostErrorKind::RuntimeError,
                                     doc: "A tool failed.".into() }],
    ..HostModule::untyped("tools", &["fetch"])
})?;
// ...at the host call:
Resume::Error(HostError {
    class: Some("tools.ToolError".into()),          // `ToolError` for the builtins module
    data: Some(HostValue::Dict(vec![("status".into(), HostValue::Int(404))])),
    ..HostError::new(HostErrorKind::RuntimeError, "not found")
})
# }; Ok(()) }
```

```python
from tools import ToolError, fetch
try:
    page = fetch("https://example.com")
except ToolError as e:            # also caught by `except RuntimeError`
    print(e, e.tool, e.data["status"])   # not found tools.fetch 404
```

- With `class` or `data` set, the exception has `tool` (the call name) and
  `data` (the payload converted to Python, or `None`). Instances built in
  Python have both as `None`.
- It is an ordinary exception object in the heap, so it survives snapshots
  and restore like any value, and so does a pending call answered later.
- An unknown `class` raises `TypeError: unknown host exception class 'x'` at
  the call: the mistake is the host's.
- Failures of the bridge itself stay plain built-in exceptions: an unknown
  name is `NameError`, a refused argument `TypeError` before the call, a
  missing OS hook `PermissionError` (filesystem) or `RuntimeError` (any
  other).

Raising is the default, and models handle it with `try`/`except`. A host
may instead return errors as values, for example `{"ok": False, "error":
...}` as Cloudflare's codemode does \[S\]; nothing in the engine depends on
the choice, but models check a returned error less reliably than they
catch an exception \[E\], so prefer raising for failures and values for
expected outcomes such as "not found" in a search.

### Values

| Python | `HostValue` | Notes |
|---|---|---|
| `None`, `bool`, `float`, `str`, `bytes` | `None`, `Bool`, `Float`, `Str`, `Bytes` | |
| `int` | `Int(i64)`, or `BigInt(String)` in decimal past `i64` | |
| `list` | `List` | |
| `tuple` | `Tuple` | a value that goes host → guest → host comes back unchanged |
| `dict` with `str` keys | `Dict(Vec<(String, HostValue)>)` | insertion order kept |
| named tuple | `Record { name, fields }` | a `Record` from the host becomes a named tuple instance |
| host class instance | `Object { class, fields }` | see [Host classes](#host-classes); fields in the class's order |
| anything else (sets, functions, instances, non-`str` keys) | none: strict mode raises `TypeError` in Python before the call | with `HostArgs { lenient: true }` it arrives as `Unconverted::{Opaque, KeyNotStr, TooDeep, TooLarge}`, and the host decides |

`mm-cell-worker`'s protocol has no tuple, so it sends a `Tuple` as a JSON
array. There are
no proxy objects whose attribute access calls the host, so every engine value
stays snapshot-safe. A handle (a job, a file revision) is a host class whose
fields hold a plain id and whose methods are host functions.

### The stdlib's OS hooks

The bundled stdlib has no ambient I/O. Its OS operations call untyped
builtins (functions of module `""`) defined under fixed names, and raise
`PermissionError` (filesystem) or `RuntimeError` naming the operation when
none is defined (`open() is not available: this sandbox has no ambient
filesystem. Use a tool for files, environment variables and commands`; the
second sentence is the `HintGroup::OsHooks` hint, so the host can name its
tools; for the host, the cause is a missing builtin of that name): `open`, the `Path.*` family (`Path.read_text`, `Path.write_bytes`,
`Path.iterdir`, `Path.scan`, ...), `os.getenv`, `os.environ`, `os.getcwd`,
`os.system` and others. These are the only dotted function names `define`
accepts. `Session::define` documents the full list and argument shapes.
Defining `os.urandom`, `time.sleep` or `asyncio.sleep` replaces the built-in
entropy, virtual sleep and virtual async sleep.

The remaining ambient state is configurable:

- `set_wall_clock(Some(Box::new(|| secs_since_epoch)))` is what `time.time()`
  and `datetime.now()` read. The default is the system clock;
  `wasm32-unknown-unknown` has none, and `None` makes the readers raise
  `RuntimeError`. `time.sleep` advances this session's clocks without waiting.
- `set_entropy(Some(Box::new(|buf| ...)))` feeds `os.urandom`, unseeded
  `random`, `SystemRandom` and `uuid4`. The default is OS randomness.
- `set_local_timezone(Some(tzif_bytes))` sets the zone for `time.localtime`
  and naive `datetime` conversions. The default is UTC. This is session
  state, so snapshots carry it.
- `set_zone_source(Some(Box::new(|key| ...)))` serves `zoneinfo.ZoneInfo(key)`:
  return the zone's TZif bytes, for example
  `std::fs::read(Path::new("/usr/share/zoneinfo").join(key)).ok()`, or
  `None` for an unknown key. Keys arrive validated as CPython validates them
  (relative, normalized, no `..`). Without a source only `ZoneInfo('UTC')`
  loads, and other keys raise `ZoneInfoNotFoundError` saying the host
  provides no time zone data. The source is not session state, but the
  zones a session loaded are: a snapshot keeps their rules, and a restored
  session answers them from its cache with or without a source. `mm`
  serves `$TZDIR` or `/usr/share/zoneinfo`.

## Driving a session

The event loop above is the core API. For the common case, a `HostHandler`
answers each stop, and `Session::drive` resumes the session until the cell
completes or raises, or the handler returns `Step::Stop`:

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default());
# let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?;
# let budget = 1_000_000_u64;
# let _ = (engine, code, budget);
# let _ = {
use micro_monty::{Call, HostError, HostErrorKind, HostHandler, HostModule, HostValue, ParkReason, Resume, Session, Step};

struct Tools { lookup: u32 }

impl HostHandler for Tools {
    fn host_call(&mut self, _: &mut Session, call: Call<'_>) -> Step {
        Step::Resume(if call.function == self.lookup {
            Resume::Value(HostValue::Int(2))
        } else {
            Resume::Error(HostError::new(HostErrorKind::ValueError, call.name))
        })
    }
    // Optional; both default to Step::Stop.
    fn waiting(&mut self, _: &mut Session, calls: &[u64]) -> Step { /* Resume::Batch */ Step::Stop }
    fn parked(&mut self, s: &mut Session, reason: ParkReason) -> Step {
        s.set_fuel(Some(1_000_000));
        Step::Resume(Resume::Continue)
    }
}

let ids = session.define(&HostModule::untyped("", &["lookup"]))?;
let mut tools = Tools { lookup: ids[0].id };
# let code = &engine.compile("lookup('apple') * 3", "cell.py")?;
let progress = session.run(&code, &mut tools)?;   // start + drive
# let Progress::Complete(Some(c)) = progress else { panic!("{progress:?}") };
# assert_eq!(c.repr, "6");
# }; Ok(()) }
```

- **The session is lent to the handler at each stop.** The handler can
  snapshot it, refuel it or look up ids before it answers.
- **`Step::Stop` hands the stop back.** `drive` returns that `Progress`
  unchanged. The host continues it by hand, or calls `drive` again.
- **A wrong answer is a `ResumeError`.** For example, `Continue` at a host
  call. The session is unchanged, still at that stop.
- **`AsyncHostHandler` with `run_async`/`drive_async`** is the same for
  async hosts, on any executor. Handler methods return `BoxFuture<'_, Step>`
  (a pinned, boxed `Send` future), so a tokio host can await a tool call or a
  channel before it answers.
- **Fuel slices keep the executor responsive.** The cell runs in slices of
  `ASYNC_SLICE_FUEL` (10,000) fuel units. When a slice runs out, the future
  wakes itself and returns `Pending`, and the next poll continues the cell
  where it parked. Pause-anywhere is what makes this work at any depth, so
  sessions on one thread interleave.
- **Slicing does not change the session's fuel budget.** The budget is
  charged exactly, including native charges that cross a slice boundary.
  Only its own exhaustion reaches `parked`.
- **A collection step ends a slice too.** Collection work charges no fuel,
  so while it slices, `drive_async` turns on `Session::set_gc_yield`: a
  slice also ends after a safepoint that ran a step of a garbage
  collection, so a poll waits for at most one collection step plus a slice.
  A host that slices with `set_fuel` itself can turn it on too; a `Fuel`
  park with fuel left is such a yield. It changes no fuel totals and no
  results, and is off by default. It is not in a snapshot: set it again
  on a restored session.
- **A CPU thread of its own.** A host that wants the cell off the executor
  can still wrap the future in its executor's `spawn_blocking`.

Neither handler needs a feature or a dependency, and neither is generic.
`drive` takes `&mut dyn HostHandler`.

## Messages for models

What a model reads after a failure decides its next attempt, so the engine
keeps CPython's wording and adds a sentence where the sandbox differs.

**Redirect messages.** An import of something micro-monty does not have
keeps CPython's `No module named 'x'` and, in the report only (not in
`str(e)`, like CPython's "Did you mean"), one sentence for its group. The
`OsHooks` sentence is the exception: it ends the refused hook's own message,
so `str(e)` carries it too.

| `HintGroup` | Covers | Default sentence |
|---|---|---|
| `DataLibraries` | `numpy`, `pandas`, `scipy`, `sklearn`, `polars`, `pyarrow`, `matplotlib`, `seaborn`, `sympy`, `networkx`, `torch` | micro-monty runs a Python subset without third-party packages; use lists, dicts and the math, statistics, csv and json modules, or a tool |
| `Network` | `requests`, `httpx`, `aiohttp`, `urllib3`, `urllib.request`, `urllib.error`, `http`, `socket`, `ssl`, `smtplib`, `ftplib`, `websockets` | There is no network in this sandbox; call a tool to fetch data |
| `Processes` | `subprocess`, `multiprocessing`, `threading`, `_thread`, `concurrent`, `signal`, `pty` | There are no processes or threads in this sandbox; run commands with a tool, and run tool calls concurrently with asyncio.gather |
| `Filesystem` | `shutil`, `glob`, `zipfile`, `tarfile`, `gzip`, `bz2`, `lzma`, `sqlite3`, `pickle`, `shelve`, `fileinput` | There is no filesystem in this sandbox; read and write files with a tool |
| `Timezone` | `dateutil`, `pytz`, `tzdata` | Use zoneinfo.ZoneInfo, or datetime with timezone.utc or fixed timedelta offsets |
| `Validation` | `pydantic`, `attrs`, `attr`, `marshmallow` | Use dataclasses |
| `Yaml` | `yaml`, `ruamel` | Use json, or tomllib for TOML |
| `HtmlParsing` | `bs4`, `lxml`, `html5lib` | Use re and str methods on the text, or a tool |
| `XmlEmail` | `xml`, `email` | micro-monty does not bundle '{module}'; use re and str methods on the text, or a tool |
| `NotEnabled` | a bundled module whose `mod-*` feature this build leaves out | '{module}' is not enabled in this build of micro-monty |
| `Asyncio` | a name `asyncio` lacks (`eager_task_factory`, ...); its streams (`open_connection`, ...) get `Network` and `create_subprocess_*`, `to_thread`, `run_coroutine_threadsafe` get `Processes` | micro-monty's asyncio has ALL_COMPLETED, Barrier, BoundedSemaphore, ... and wait_for; pass coroutines to asyncio.gather (the list is generated) |
| `OsHooks` | a refused OS hook (`open`, `Path.*`, `os.environ`, `tempfile`) | Use a tool for files, environment variables and commands |

A module belongs to a group by its exact dotted name or its top-level
name. `Session::set_hint(group, Some(text))` replaces a sentence
(`{module}` stands for the module), `Some("")` drops it and `None` restores
the default; for example a code-mode host might say "use the nested tools". The choice
is session state: it survives `reset` and snapshots. When the stdlib
gains a module (as `zoneinfo` did), the import succeeds and the hint never
shows.

**Did you mean.** A missing module attribute gets CPython 3.14's
suggestion (`AttributeError: module 'json' has no attribute 'lods'. Did
you mean: 'loads'?`), for bundled and host modules alike, and so does
`from json import lods`. When nothing is close in `asyncio`, the `Asyncio`
hint follows instead.

**Coroutines never awaited.** When a cell completes or raises, the engine
scans the coroutines the cell created and the host futures it was given
(`Resume::Future`) and writes CPython's warning to stderr for each one never
started, scheduled or awaited: `<sys>:0: RuntimeWarning: coroutine 'fetch'
was never awaited`, once per name (its `__qualname__` when created), in
creation order, coroutines before host futures. Coroutines passed to
`create_task` or `gather` count as scheduled even if their task never
runs. It is a scan at the end of the cell, not a finalizer: the list does
not keep coroutines alive, and one collected unstarted leaves its name, so
the report does not depend on GC timing (spec §1). Unlike CPython it does
not wait for the object to die, so a coroutine kept in a global for a later
cell is reported too, and the location is always `<sys>:0` with no source
line or tracemalloc note. Cancelled cells report nothing.

**What already ran.** `Session::call_log()` lists the host calls of the
running cell, or of the last one after it completed, raised or was
cancelled, as `HostCallRecord { call, function, name, status }` in call
order. `CallStatus` is `Returned`, `Raised`, `Pending` (the cell is stopped
at it or waits on its future) or `Abandoned` (the cell ended before an
answer, for example an unawaited future). `start` begins a new log, and
snapshots carry it. A host can tell the model "these calls ran before the
failure and are not undone", as pi and Pydantic AI do \[S\].

**Environment summary.** `Engine::environment()` returns a few lines for a
prompt, generated from the build: the modules that import, the bundled
ones this build leaves out, what is never available, the asyncio surface,
the engine's limits and the conventions above (sync calls, `await` and
`gather` for futures, `TimeoutError`). It never drifts from what the build
supports. Add `Session::stubs()` for the host's own functions.

**Long results.** A completion's `repr` and `str` are cut at 1 MiB by default
(the memory limit if that is less), about what a code-mode host passes to a model;
`set_text_limit` raises or lowers it. `set_text_cut(TextCut::HeadTail)` keeps the beginning and
the end of a long completion text around `... [N bytes omitted] ...`. A
value that converts to a list, tuple, dict or record keeps its shape
instead: `[{'id': 0}, {'id': 1}, ...(+998 items)]`, with long strings
inside cut as `'abc'...(+N chars)`. `Completion::value` keeps the whole
`HostValue`, so the host can store it and hand the model a reference.
Rendering a container stops at eight times the text limit (at least 1 MiB):
a longer text that the shaped cut cannot describe keeps its beginning and
ends in ` ... [more omitted]`, since its end was never rendered. With
`TextCut::Head` rendering stops at the limit itself.
`micro_monty::head_tail(text, max_bytes)` applies the same cut to output a
host collected (OMP keeps 50 KiB of head and tail \[S\]).

**Long exception reports.** An uncaught exception's report is bounded the
same way. Rendering `PyException::message` (its `str`) and
`PyException::traceback` stops once either passes the text limit, as a
completion's `repr` does (eight times the limit, at least 1 MiB, with
`HeadTail`), and stops when the session's deadline has passed during long
rendering (a chain or exception group of more than 64 exceptions, or a
`repr` walk). Each text is then cut as `set_text_cut` says: its beginning
(`Head`), both ends around `... [N bytes omitted] ...`, or, when rendering
stopped, its beginning and ` ... [more omitted]`. `truncated` is `true`
when either text was cut; a cut traceback may lack its final newline. A
report under the limit is exactly CPython's, with `truncated` false. So an
exception whose message is a huge container (a frozenset sharing itself
24 levels deep has a 2²⁴-fold `repr`), or a group that shares sub-groups,
ends the cell promptly with at most the text limit per field, not after minutes
with a report larger than any frame a host would send \[M\].

## Parking, deadlines and cancel

A cell parks at a safepoint: a call, a backward jump, a step of a long
native operation, or a check the compiler puts in long straight-line code.
A return is not one: the returns between two safepoints are at most the
frames on the stack, each a few instructions. The engine looks at the
fuel, the deadline and park requests every 256 safepoints, or at every
one once less fuel than that is left, so a park request or deadline is
noticed within 256 calls or loop iterations, and `Session::fuel()` read
while the cell runs (from a host call) is the fuel as of the last look,
up to 255 units more than is left. At a park it is exact. That works
at any depth and in any frame kind, including generators, coroutines,
dunders and comprehensions. Every frame stays in the arena, so parking costs
nothing and `Resume::Continue` continues exactly where the cell stopped.

| Reason | Trigger |
|---|---|
| `ParkReason::Fuel` | fuel ran out. Call `set_fuel(Some(n))`, then `Continue`. With `set_gc_yield(true)`, also after a collection step, with fuel left (`fuel()` above 0): `Continue` as is |
| `ParkReason::Deadline` | `set_deadline`'s instant passed |
| `ParkReason::Requested` | `ParkHandle::request()` from any thread (`session.park_handle()`, `Send + Clone`), or `request_park()` |

`Session::cancel()` ends a stopped cell (parked, at a host call or waiting)
without running more of it. No `finally` blocks run and pending futures are
dropped. Globals keep what the cell assigned, so the session stays usable. To
stop a cell running on another thread, request a park, then cancel the
parked cell.

A park request stays until a cell parks for it. Neither `start` nor
`cancel` drops it: a request made between cells, or while a cell is
stopped, parks the next cell at its first safepoint. So a host that
requests a park and then sees the cell end on its own (it completed, or
stopped at a host call) gets one more `Parked { reason: Requested }` from
the next cell, and answers it with `Continue` when it no longer wants it.
`Session::phase()` says what the session is stopped at (`Idle`,
`HostCall`, `Waiting` or `Parked`), and so which `Resume` it takes;
`ResumeError::Unexpected { phase }` reports it too. `Session::pending()`
re-reports the stop itself, as `start` or `resume` reported it:
`Progress::HostCall` with its call id, function, name, arguments and
keyword arguments, `Progress::Waiting` with the calls still pending, or
`Progress::Parked` with its reason; `None` between cells. Snapshots carry
it, so a restored session (even in another process) says which call it
waits for and with what arguments, and the host need not keep the payload
beside the image. The session keeps a copy of a host call's arguments
until the call is answered.

## Embedding guarantees

What a host running sessions in its own process can rely on, each backed by
a test. Numbers are for x86_64 Linux release builds unless stated.

**By target.** What bounds a cell's CPU and memory on each place it runs
(spec §5.1, "The bounded-work contract for natives", and §11; at eb78933):

| | Native (the facade) | wasmtime (`mm-wasm-host`) | Node (`hosts/js`) | Cloudflare Workers (`hosts/js`) |
|---|---|---|---|---|
| Longest step between two safepoints | 50 ms at the default limits on every `Bounded` row of `tests/steps.rs` (349 rows, none `Open`; the exceptions in spec §5.1 are not bounded yet) \[M\] | not measured per site; about 1.5–3× native \[E\]. A park request not honoured within 100 ms + 0.75 ms per MiB of the memory limit (292 ms at 256 MiB) traps the instance | no time bound: a step costs its fuel units times the time per unit below. On 8cd850c the longest steps were 43–132 ms at a 16 MiB limit and up to 2.1 s at 256 MiB \[M\] | as Node (workerd within 20% of it) \[M\] |
| Fuel overdraft per slice | at most 2^21 + 2^17 units on every `Bounded` row, unless the operation parked first with `fuel_wanted` and the host granted it \[M\] | the same: fuel counts are identical across targets | the same | the same |
| Deadline | `set_deadline`: noticed at the next check (every 256 safepoints), never inside one step | the same, on the host's clock; plus the grace trap above | the same, on `performance.now()`; nothing stops one long step but the platform's own limit (`Worker.terminate()`) | never fires while a cell runs: the clock stands still during synchronous wasm, so fuel is the only CPU bound |
| Memory, `Limits::memory` = L | Rust heap at most 2.5 × L + 16 MiB while a cell runs (2.01 × L measured) \[M\] | linear memory about 3 × L + 16 MiB, which also covers a snapshot; it never shrinks | as wasmtime; the host sets no store limit, so the arena limit is the cap | as Node |
| Time per fuel unit, bench programs | 0.014–0.25 µs \[M\] | 1.5–2.2× native's, 0.032–0.47 µs (`mm` for `wasm32-wasip1`, eb78933) \[M\] | 0.024–0.38 µs \[M\] | 0.029–0.45 µs in workerd \[M\]; a whole code-mode cell about 0.85 µs, most of it compiling, imports and host calls |

**wasmtime, known issue in 0.0.1.** Since 4fe04e1, Cranelift spills the
linear-memory base pointer in `Vm::execute` and reloads it before each
opcode fetch, so the dispatch loop runs slower under wasmtime than the
builds before: guest instructions +8.0% on fib, +10.2% on closure and
+6.3% on generators, wall time 1.055, 1.087 and 1.042 × \[M\]. V8 and native
are unaffected by the spill. The last batch before 0.0.1 moved code
layout again, with guest instructions up only 1% on fib and closure: on
x86_64 hosts that compile for their own CPU (AVX-512), wall time against
7ac25ef is now 1.105 on fib, 1.168 on closure, 1.097 on classes and
1.021 on generators under wasmtime, and fib and closure 5–11% slower
under V8 and the V8 bundle \[M\]. Whether the register allocator keeps
the base in a register, and how the loop lays out, depend on unrelated
code in the function, so the fix waits for 0.0.2 (HANDOFF, "0.0.2
list").

Per-unit time varies by native: bytecode costs 0.03–0.05 µs a unit in
workerd, and the natives charged most cheaply (`str.upper`, `str.find`,
`int(str)`) up to 0.48 µs (docs/research/workers-v8.md §8, on 8cd850c).
So a budget of F units bounds a cell's CPU on V8 to about F × 0.5 µs plus
its fixed part (compiling, imports and host calls, up to about 8 ms for
the 22 code-mode tasks), within the step and overdraft bounds above.
Results do not depend on how a host slices the run: output, exceptions
and values are the same unparked, in fuel slices of any size and across
restores on either target (`cargo xtask durability`, `--cross-target`),
except near the memory limit, where a snapshot's collection can decide
whether an operation raises `MemoryError`. The fuel spent agrees within
0.2% across slices of 1 to 30,000 units \[M\], and after restores except
as listed in [Other differences from CPython](#other-differences-from-cpython):
a snapshot inside an out-of-memory retry wait, and a long non-ASCII str
indexed after each restore (43,313 units unparked against 8,687,009 with a
restore at every 20,011-unit park \[M\]).

**Panics: build with `panic = "unwind"`.** No guest input should panic:
clippy denies `unwrap`, `expect`, `panic!` and `todo!` in engine crates,
and eleven libFuzzer targets (`fuzz/fuzz_targets`) look for the rest. If an engine bug
panics anyway, what happens depends on the build. With unwinding, the
facade catches it, that session reports an internal error from then on
(it is poisoned), and the host and every other session keep working
(`tests/panics.rs`). The workspace's release profile uses
`panic = "abort"`, and the shipped size builds use
`panic=immediate-abort` (spec §6); either kills the whole host on such a
bug, with no message under `immediate-abort`. The isolation study
(`docs/research/isolation-modes.md`) measured both. So an in-process
embedder sets in its own `Cargo.toml`:

```toml
[profile.release]
panic = "unwind"
```

Builds for a wasm runtime or a child process, where the runtime or process
boundary contains the fault, can keep the abort builds.

**A bounded Rust stack.** Running code keeps its nesting in the arena, not
on the Rust stack: Python calls (spec §8.2), and `==`/ordering, `repr`,
`hash`, `copy`, `json` and host-value conversion on nested or cyclic data,
and `next()` of `enumerate` and `zip` nested in each other, which walk on
explicit stacks and raise `RecursionError` at the depth limit.
Parsing and compiling recurse natively, bounded by micro-monty's nesting
limit of 200 levels (see [Code size limits](#code-size-limits)); flat
chains such as `a + b + ...`, `x.f().g()...`, conditional-expression chains
and `elif` arms are walked in loops at any length. Minimum host thread
stacks \[M\]:

| Work | Release, native | Debug, native | `wasm32-wasip1` under wasmtime |
|---|---|---|---|
| Running a session, deepest data shapes | 40 KiB | | default stack |
| Compiling source at the nesting limit (`Engine::compile`, and `ast.parse` inside a cell with `mod-ast`) | 650 KiB | 3.7 MiB | default stack |

The mm-wasm guest's own shadow stack, in its linear memory, is 832 KiB
(Rust's default is 1 MiB). Its deepest legal input, nested lambda defaults
at the nesting limit, takes 670 KB \[M\], and every own case, corpus file
and bench program at most 41 KB. `crates/mm-wasm-host/tests/stack.rs` runs
the deepest inputs on painted stacks and fails past 90%; an overflow would
trap and lose the instance. An idle guest session is 1.57 MB of linear
memory (24 pages) instead of 1.77 MB \[M\].

Give threads that run sessions at least 256 KiB and threads that compile at
least 1 MiB in release builds (Rust's default for spawned threads is 2 MiB).
A session that only runs precompiled `Code` (and no `ast.parse`) does not
compile. Tests: `deep_and_cyclic_comparisons_fit_a_small_host_stack`,
`comparing_cyclic_lists_of_deep_frozensets_raises_promptly` (`tests/session.rs`)
and `deepest_host_arguments_convert_on_a_small_host_stack`
(`tests/host_calls.rs`) run on 256 KiB threads; `cargo xtask wasm` runs the
deep comparisons and the nesting-limit compiles under wasmtime's defaults
(`examples/roundtrip.rs`).

**Guest-driven allocations are charged to `Limits::memory`.** The arena,
retained code, frames and task stacks, scheduler tables, instance shapes,
collector scratch, the comparison walk's spill stack, host-module definitions,
and the `HostValue` trees built for host-call arguments and completion values
all reserve from the session budget before they allocate, and fail with
`MemoryError` (or `TooLarge` for a lenient host argument) instead of growing
(spec §11; `host_value_conversion_is_charged_to_the_session_memory_limit`).
Compiling a cell with `Session::compile` is charged too: the session
reserves 512 bytes per source byte + 16 KiB while it parses and compiles
(the worst measured is about 405 \[M\]), and a cell too big for the memory
left is a `CompileError` whose `type_name` is `MemoryError`. Not charged,
but bounded by something else: `Engine::compile` (by the 256 KiB source
limit), snapshot buffers (by `snapshot_with_limit`), output text (by
your output callback) and the temporaries of one bigint operation (checked
against the space left before it starts). The allocator's transient peak
while a buffer grows is not counted either, so leave headroom above the
limit for process RSS, as below.

**The Rust heap stays within 2.5 × `Limits::memory` + 16 MiB while a cell
runs, for any guest input.** That is the arena (at most the limit), at
most about one transient native copy of the largest value a native reads
(a `str + str`, a repeat, a search table), and fixed tables and scratch.
Natives whose buffers the guest could make larger than that admit them
against the limit first (spec §11). The measured worst case is 2.01 × the
limit \[M\], over 73 probes at 16, 64 and 256 MiB.
`cargo xtask check` replays those probes with a counting allocator
(`fuzz/memory_probes`), and the `memory_bound` fuzz target searches for
inputs that break the bound. On top of that bound:

- **`Engine::compile`** is outside the limit: at most 512 bytes per source
  byte + 8 MiB \[M\], about 136 MiB at the 256 KiB source limit. Compile
  cells a model or user writes with `Session::compile`, which charges it.
- **Snapshots:** writing or restoring one holds the image once on top of
  the arena: at most the limit + the image + 16 MiB \[M\]. A pending host
  call's arguments, which the session keeps for `pending()` and the image,
  are charged to the session.

Size the process from this. For a wasm host, give the instance about
3 × the session limit + 16 MiB of linear memory, which covers the run bound
and a snapshot. The guest frees an input or output buffer larger than
64 KiB once it has been read, so a snapshot's or restored image's space goes
back to the session's heap (linear memory never shrinks).

**The module cache is the Engine's, charged once.** The first time a session
of an `Engine` imports or restores a bundled Python module, it decodes the
module's bytecode, and the Engine keeps the result; the same holds for the
prelude chunks its built-in classes and functions (asyncio's, for one) are
unpacked from. Every later session of that Engine and its clones reuses
them, from any thread. A session's `Limits::memory` still covers its own
arena and its code tables. On a cache hit it is also charged the decoder
workspace that module or chunk took. A session is
therefore charged the same whether the cache is cold, warm or off: the same
`MemoryError`, `TooLarge` and output in each case
(`the_module_cache_stops_at_its_limit_and_sessions_decode_past_it`,
`sessions_on_many_threads_share_decoded_modules_and_behave_as_uncached`,
`prelude_chunks_from_the_cache_are_charged_as_unpacked` in
`tests/sessions.rs`).

- **What the Engine adds:** the decoded programs themselves, outside every
  session's limit. They grow to at most the cap,
  `Engine::with_module_cache(config, max_bytes)`. `Engine::new` uses
  `DEFAULT_MODULE_CACHE` (8 MiB), and every bundled module decoded is about
  2.9 MB \[M\].
- **Past the cap**, a session decodes (or compiles host code) and keeps
  its own copy, as without a cache, and its images store that host code.
  Nothing is evicted.
- **Turning it off:** `max_bytes = 0`. (The mm-wasm guest also leaves
  unpacked prelude chunks out of its engine's cache, since its one session
  lives as long as the instance's linear memory.)
- **Reading its size:** `Engine::module_cache_bytes()` reports what the
  cache holds, host-module code included (about 0.67 MB per 100 typed
  functions \[M\]; raise the limit for large tool sets).

**Host-module code is cached the same way.** The code generated for a
host module's typed functions and classes ([Host functions and host
modules](#host-functions-and-host-modules)) is compiled by the first
session of the Engine that defines the module and kept under the same
limit, by a key that hashes the generated source. A session is charged the
same for it with the cache cold, warm or off, since compiling was never
charged. Images refer to cached host code by its key, and any Engine of
the build restores them, compiling the code again if its cache lacks it.
The bundled modules' part of the cache does not change snapshots, build
ids or restore: an image from a cached Engine restores on an uncached one.

Measured on the codemode set (json, asyncio, re, collections, datetime) \[M\]:

- warm restore 9.95M Ir, against 23.75M without the cache;
- warm fresh imports 28.6M Ir, against 42.6M;
- RSS per session with 40 held: 1.2 MB, against 1.85 MB.

The first session in a process pays the full decode. A process with one
session, such as a one-session worker process or an mm-wasm instance, gains nothing
(bench/evidence/results/2026-11-07-module-cache).

**Bound CPU with fuel slices.** Run cells with a fuel budget and resume them
slice by slice: on `Parked { reason: Fuel }`, read `Session::fuel_wanted()`
before granting more. It is non-zero when the next native operation (a large
int `*`, `**`, `/`, `//` or `%`, a big-int builtin such as `pow`, or
`sorted`/`list.sort` of a long list; `Session::fuel_wanted`'s documentation
lists them all) parked before starting because it needs more
fuel than was left; resuming runs it whatever fuel is then set, so a host that
enforces a budget stops the cell instead when the wanted fuel is more than the
cell may still spend. Any other native operation runs in bounded steps
(docs/spec.md §5.1): a slice of fuel is overdrawn by at most 2^21 + 2^17
units, about one 20 ms step \[E\], and a park request or deadline waits at most
about 50 ms for the step to end \[M\]. `crates/micro-monty/tests/steps.rs`
checks both on every site it lists, all of them bounded at 0.0.1.
Outside that bound: full garbage collections over a large heap, about
0.4 ms per MiB live natively and 1.3–1.6 times that on wasm \[M\]: one a
native or the engine requests, a snapshot's and code reclamation's (docs/spec.md §5.1, §9.2). In
a wasm embedding the reference host's grace interrupt (100 ms after a park
request plus 0.75 ms per MiB of the memory limit, configurable) is the last resort, and it costs the instance.
On V8 (Node, Cloudflare Workers) there is no such interrupt, and on
deployed Workers the clock stands still while wasm runs, so fuel is the
only CPU bound there.

**A snapshot or restore is not a step.** `Session::snapshot` collects in
full first and cannot park: about 1.4 ms per MiB of image natively, under
wasmtime and on V8 alike (about 250 ms at 170 MiB) \[M\]. A restore reads
the whole image and costs about as much per MiB \[M, eb78933, one 16-core
Amp orb; other machines differ; `bench/evidence/restore-cost/run.sh`
reruns them\]:

- a heap of small objects (35–169 MiB of two-int lists): 1.2–1.5 ms per MiB
  natively, 1.0–1.8 under wasmtime 50 and 0.9–1.7 on V8 (Node 26), about
  200–300 ms at 169 MiB;
- large blocks (a long `str` or `bytes`): 0.6–0.9 ms per MiB natively and
  0.3–0.7 under wasmtime and on V8;
- the first restore in a process, which a fresh worker pays: up to 1.7 ms
  per MiB natively and 1.8 on wasm (280–300 ms at 169 MiB). On V8 the first
  call into a new instance also warms up: 154 ms for a 35 MiB image.

The room an image records past a large block (a `json.dumps` text growing in
place) is reserved again, not filled, and adds nothing measurable: a restore
that commits 43 MiB beyond its 72 MiB image costs what one without room
does \[M\]. A park request or deadline does not stop either call, the
reference wasm host does not apply its grace interrupt to a snapshot (only
the opt-in `call_budget`), and a restore runs before its session's park
handle exists. Do not cover these calls with a host timeout: cutting one
loses the image, and in wasm the instance. On Workers the platform's CPU
limit still applies, so leave room for the largest image a session can
make.

**Count the fuel a cell was charged, not what you granted.** A native that
charges after it runs can take more than the slice it started in, and
`Session::fuel()` then stops at 0, so granted fuel less what is left misses
the overdraft: a loop of `str.replace` calls on a few MB passes any budget
counted that way. Set a fuel total (`set_fuel_total`, which every charge
under a fuel limit comes off, overdrafts included; refuel with `Some(n)`,
not `None`) and count from it; `hosts/js`'s driver does.

**What a fuel unit costs, by target** \[M\]. Per charged unit, in 10,000-unit
slices; the native build is the `mm` binary, the wasm columns the shipped
mm-wasm guest (docs/research/workers-v8.md §8, `8cd850c`):

| | x86_64, native | V8, Node 26 | workerd |
|---|---:|---:|---:|
| Bytecode (`loop`, `closure`, `fib`, `generators` in `bench/`) | 0.014–0.026 µs | 0.024–0.041 µs | 0.029–0.044 µs |
| All 12 bench programs | 0.014–0.25 µs | 0.024–0.38 µs | 0.029–0.45 µs |
| A code-mode cell (22 tasks; mostly compile, imports and host calls) | | 0.86 µs | 0.85 µs |
| The dearest native per unit (`str.upper`) | | 0.41 µs | 0.48 µs |
| One native step at a 16 MiB limit, at most (`str.translate`) | | 102 ms | 132 ms |
| … at the default 256 MiB | | 1.7 s | 2.1 s |
| A garbage collection (no fuel since 642fdb3; measured on 8cd850c) at 16 MiB / 256 MiB | | 5 / 72 ms | 5 / 85 ms |

So CPU per cell is at most about a fixed part (compiling, imports: up to
8 ms for the 22 tasks) + 0.5 µs × the fuel budget + one native step + a
collection, on V8 \[E\]. The step is the term fuel does not bound: it
follows the largest object the memory limit admits.

**A wasm embedding.** `crates/mm-wasm-guest` builds micro-monty as a wasm
module (`wasm32-unknown-unknown` or `wasm32-wasip1`) speaking the `mm-wasm`
interface (`crates/mm-wasm-abi`, `ABI_VERSION` 1), with one session per
instance; `crates/mm-wasm-host` is a synchronous wasmtime reference host
with an API close to `Session`, and `hosts/js` a JavaScript host for V8
(Node, Workers) with the same API and K sessions per instance as an opt-in.
Snapshots move between native and wasm sessions. A session's options (its
limits and the settings an image does not carry, as `SessionSettings`) are
a tagged record that `mm_session_new` and `mm_session_restore` both take.
See `docs/research/wasm-embedding.md`; the interface is frozen with 0.0.1:
a release may add a session option or an optional export, and anything
else bumps `ABI_VERSION`.

Download sizes of the shipped `wasm32-unknown-unknown` builds (`cargo
xtask size`: release, `immediate-abort`, `wasm-opt -Oz`; at e1328bf, gzip
1.12 and zstd 1.5.4) \[M\]:

| Module | Bytes | `gzip -9` | `zstd -19` |
|---|---:|---:|---:|
| `hello.wasm` (the facade's example) | 2,271,773 | 1,113,696 | 980,776 |
| `mm_wasm_guest.wasm` | 2,358,676 | 1,147,972 | 1,006,224 |
| `mm_wasm_guest_bundle.wasm` | 2,357,103 | 1,146,092 | 1,005,668 |

**An allocator for `wasm32-unknown-unknown` (optional).** There, std's
allocator is the `dlmalloc` crate, whose `free` and `realloc` check the
layout with two assertions that fat LTO inlines into every deallocation
site: about 84 KB of `hello.wasm` and 97 KB of the guest \[M\].
`crates/mm-wasm-alloc` is the same dlmalloc, at the version std uses,
called through its layout-free `c_free`/`c_realloc`, without the checks.
The facade never registers it (a binary has one global allocator, and the
choice is the embedder's). To use it, depend on `mm-wasm-alloc` (in this
repository, `publish = false` for now, so a git or path dependency) and
register it in the binary:

```rust ignore
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[global_allocator]
static ALLOCATOR: mm_wasm_alloc::Allocator = mm_wasm_alloc::Allocator::new();
```

The crate is empty on every other target. It is for single-threaded
modules: without the `atomics` target feature, as rustc builds
`wasm32-unknown-unknown` by default. On `wasm32-wasip1`, std uses
wasi-libc's `malloc`, which has no such checks. The crate goes away once
the pinned toolchain's std ships a dlmalloc whose check is a debug
assertion (docs/spec.md §5).

**Fuel in every guest-driven native loop.** Every loop whose length a guest
controls charges fuel in proportion to its work, so an exhausted budget,
deadline or park request takes effect at the next safepoint
(`native_work_observes_control_without_a_fuel_limit`,
`comparing_exponentially_shared_data_parks_on_fuel_and_on_a_deadline`,
`a_catastrophic_regex_parks_on_fuel_and_on_a_deadline` in
`tests/parking.rs`). An operation that is not chunked runs to completion
once started, so its cost bounds the park latency; chunked ones park
between steps of at most 2^17 units (docs/spec.md §5.1). The int operators `*`, `**`,
`/`, `//` and `%` check their cost first: one that would charge 65,536
units or more, more than the fuel left, parks before starting, and
`Session::fuel_wanted()` reports its cost (a snapshot keeps it).
`Resume::Continue` then runs it whatever the fuel, so a host enforcing a
budget ends the cell instead
(`an_int_operation_the_fuel_cannot_pay_parks_before_it_starts`). The
big-int builtins called directly from Python code check the same way:
`pow` (two and three arguments), `divmod`, `sum` and `math.prod` of a
list or tuple, and `math.factorial`, `perm`, `comb`, `isqrt`, `gcd` and
`lcm` (`big_int_builtins_the_fuel_cannot_pay_park_before_they_start`). The
same builtins called by another native (`map(pow, …)`, a `key=` function)
and `sum`/`prod` of any other iterable run to completion once called,
charging their fuel afterwards, so the next safepoint parks. Whatever
the fuel, ints over 2^26 bits, results beyond the memory limit and int
operations estimated to run past about 45 ms in one step (a product of two
3-million-bit ints, `3 ** (2 ** 22)`, `math.factorial(50_000)`) are
refused up front with `MemoryError` (`integer operation too large` for
the last), without a fuel park first, so no int operation holds a
deadline back for more than about 50 ms.

`repr` and `str` of a container follow the comparisons' pattern. Inside a
cell (`repr()`, `str()`, f-strings, `format`, `%`, `print`) the native
walk does at most 2^17 fuel units of work: its visits, and each big
int's decimal text by its size, checked before the int is converted (as
`%` formats, `str.format`, `print` and `sorted(key=str)` do), so a step
holds a step's work or one conversion; a long str or `bytes` in it is
rendered a step at a time. Past
that it charges what it visited and the prelude's `_repr_of` renders the
rest level by level, with a native walk per item, so a graph of shared
containers describing gigabytes of text parks on fuel instead of running to
the memory limit in one step, with fuel limited or not. The completion's `repr`/`str` after the cell cannot run Python;
it stops at what the text cut keeps (see Long results)
(`a_long_container_repr_inside_a_cell_parks_on_fuel`,
`a_cut_completion_repr_stops_walking_past_what_the_cut_keeps`,
`vm_bytecode_slow_seeds_finish_promptly` in `tests/session.rs`). Error
messages that show a container's `repr` (`create_task`, duplicate match
keys) render it whole.

### Many sessions in one process

The facade registers no global allocator and starts no threads, so a host
running many sessions in one process chooses both. On glibc's `malloc`, one
thread per session is the costly pair: each thread that allocates gets an
arena of its own (up to 8 per core), and freed memory stays in it. A soak
test (64 sessions in one process, each on its own thread, replaying
recorded plak cells with about 14 MB of live engine heap among them)
measured \[M, eb78933, one 16-core Amp orb\]:

| Host allocator | RSS under load | RSS with no sessions left | Per-cell p99 |
|---|---:|---:|---:|
| glibc | 702 MiB | 609 MiB | 4.1 ms |
| glibc, `MALLOC_ARENA_MAX=2` | 333 MiB | 242 MiB | 280–400 ms |
| glibc, trim and mmap thresholds 128 KiB | 280 MiB | 53 MiB | 230–250 ms |
| mimalloc | 418 MiB | 48 MiB | 3.8 ms |
| jemalloc | 385 MiB | 35 MiB | 3.8 ms |

This is memory kept, not leaked: over 8 hours on glibc the hourly RSS
stayed within 1% (722–731 MiB), and 517 MiB stayed resident once every
session had closed. Fewer arenas make the threads queue for them, and low
trim thresholds hand back pages the next cell faults in again, so either
tuning costs p99 50 to 100 times over. Instead, register mimalloc or
jemalloc as the host binary's `#[global_allocator]`, or run sessions on a
fixed pool of threads (a session stopped on one thread resumes on another),
so that the number of arenas follows the pool and not the session count.

The reference wasm host (`crates/mm-wasm-host`, wasmtime's default on-demand
instance allocator) gives every session and every restore a new instance
whose linear memory starts zeroed, out of the host allocator's sight. With
many such sessions in one process, a snapshot and restore onto a new
instance costs far more than natively \[M, 4b32aa2, 0.4 MiB images, one
16-core Amp orb\]: 4.9 ms per swap with 8 sessions and 10 ms with 32 over a
minute; with 64 the cost climbs for the first minutes and averaged 144 ms
over 10 minutes (200–250 ms after the first minute; about 230 ms over an
hour on eb78933), against 1.6 ms natively. Throughput held (cells per minute matched
native's) and every restore matched. The cost is page faults: each new
instance faults its linear memory in again on first touch, 134–265
thousand minor faults a second at 64 sessions; reusing the mappings
without keeping their pages resident costs as much \[M\]. Why each fault costs
so much more with many threads in one address space was not profiled \[E\].
With many concurrent wasm sessions, spread them over processes (with 8
sessions per process a swap costs about 3 times native's), or turn on the
pooling allocator, which is off by default because it raises peak RSS:

```rust ignore
let config = RuntimeConfig {
    pool_instances: Some(128),
    pool_keep_resident: 4 << 20,
    ..RuntimeConfig::default()
};
```

It keeps `pool_instances` slots of linear memory and the first
`pool_keep_resident` bytes of each resident between instances. Size it near
the most instances alive at once: about twice the sessions that swap,
since a restore's new instance exists before the old one is dropped. Past
that, starting or restoring a session fails with `Error::Trap` (wasmtime's
instance or memory limit in its text) before any guest code runs: the image
and any live session are unchanged, and the call can be retried once a
session is dropped. Drop the old session only after its restore succeeds.
At 64 sessions over 10 minutes
\[M\]:

| Host | Per swap | Peak RSS |
|---|---:|---:|
| on demand (default) | 144 ms | 799 MB |
| 128-slot pool, 4 MiB kept | 15.3 ms | 1,153 MB |
| 1000-slot pool, 4 MiB kept | 36.8 ms | 1,611 MB |

Minor faults fell to 0.6–4.7 thousand a second. An oversized pool loses the
gain over time: the 1000-slot pool climbed from 9.6 to 94 ms per swap over
the 10 minutes. Keeping 2 MiB gave 8.1 ms per swap and 8 MiB 4.2 ms at a
1.5 GB peak (1-minute runs \[M\]); keeping none gains nothing. Even at 128
slots the cost rises after about 6 minutes (6–12 ms, then 20–38 ms) while
the host's own heap grows from 315 to 468 MiB; that is not resolved \[E\].

## Snapshots

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default().with_limits(Limits::default().with_fuel(Some(1_000))));
# let session = &mut Session::new(engine)?;
# let code = engine.compile("n = 0\nwhile n < 5000:\n    n += 1\n", "loop.py")?;
# assert!(matches!(session.start(&code), Progress::Parked { reason: ParkReason::Fuel }));
# let _ = {
let bytes: Vec<u8> = session.snapshot()?;     // any time the session is not running
// Host-side settings are not saved: give them to the restore.
let mut settings = SessionSettings::default();
settings.output = Some(Box::new(|_, t| print!("{t}")));
let mut session = engine.restore_with(&bytes, settings)?;  // any process, same build id
let progress = session.resume(Resume::Continue)?;  // or whatever the cell was stopped for
# assert!(matches!(progress, Progress::Parked { reason: ParkReason::Fuel }));
# }; Ok(()) }
```

A snapshot is the session's FULL image: a GC runs, then the heap's live runs,
code, frames, tasks, pending host calls with their ids, globals, limits and
settings are written. Restore validates every structure before use (bounds,
block headers, bytecode, frame states) and charges the result against the
restoring engine's memory limit (`TooLarge`). A parked loop's snapshot is
about 42 KB, a fresh session's 41 KB; the wasm round-trip example's, with
`re`, `random` and `time` loaded, is 282 KB \[M\].

Images are not compressed; compress them where you store or send them
("Storing snapshots" below). Measured on this build's images \[M\]
(`bench/evidence/results/2026-11-04-full-compression/`): zstd level 1 shrinks
the parked own-conformance cases 3.6× (median 71 KB → 21 KB, 0.18 ms to
encode) and retained state of 10k / 100k small dicts 17.5× / 19.2× (13.9 MB →
722 KB, 16 ms encode, 11 ms decode). zlib level 1 gives 3.3× / 9.2× / 9.7× at
about 2.5× zstd's CPU time.

The host still owns some state, because no engine can make it durable:

- Output: what was printed before the snapshot stays printed. A restored
  session's output goes where its `SessionSettings` say (nowhere by
  default).
- In-flight host calls: a restored session waits for the same `call` ids,
  and `pending()` reports the call it is stopped at, with its arguments.
  Whether the side effect already happened is the host's to know.
- Host-side settings: the output callback, the clock and entropy
  callbacks, the zone source, the text cut, `int_bits_text` and GC stress
  are a [`SessionSettings`] value, which `Session::new_with`,
  `Engine::restore_with` and `SessionTemplate::instantiate_with` take:
  build it in one function and pass it to each, and a restored session
  behaves like the saved one (`restore` and `instantiate` use the
  defaults: platform clock and entropy, no output, no zone source). The
  deadline and the park handle are per run: set the deadline again and
  take the handle again.
- Work done after the snapshot is redone after a restore.

### Storing snapshots

The engine only writes FULL images and will not grow a delta format. Make
them small where you store them:

1. **Get the base image once per build:** a fresh session's snapshot, with
   nothing run in it.

   ```rust
   # use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
   let base = Session::new(&Engine::new(EngineConfig::default()))?.snapshot()?;
   # assert_eq!(base, Session::new(&Engine::new(EngineConfig::default()))?.snapshot()?);
   # assert_eq!(&base[10..42], build_id().as_slice());
   # Ok(()) }
   ```

   It is about 41 KB and byte-identical across processes, native,
   `wasm32-wasip1` and `wasm32-unknown-unknown` for the same build \[M\]; on
   mm-wasm, `mm_session_new` with default options then `mm_snapshot` gives
   the same bytes. Store it, or regenerate it at start-up.
2. **Compress each image with zstd level 1, the base as a raw prefix and the
   frame checksum on**: `ZSTD_CCtx_refPrefix` / `ZSTD_DCtx_refPrefix` in
   libzstd, `zstd -1 --check --patch-from=base.mmsnap` on the command line.
   Decompress with the same base. On small parked sessions this is 1.3–1.4×
   smaller than zstd-1 alone (codemode median 102.8 KB → 19.7 KB); on large
   heaps it is the same as zstd-1 \[M\].
3. **Store with each blob:** the build id (`micro_monty::build_id()`,
   `mm_build_id`), which names the base, and how it was compressed (prefix,
   and dictionary id if you use one). A raw-prefix frame does not record its
   prefix.
4. **Optional:** a zstd dictionary trained on your own parks, used together
   with the base, reached 8.1× against FULL on held-out codemode parks,
   against 4.0× for zstd-1 alone \[M\]. Version it per build like the base.

Checks:

- A base belongs to the build whose id is in its bytes 10..42 (after the
  8-byte magic and the 2-byte format version). Compare that with
  `build_id()` before using a stored base, and keep a hash of the base next
  to the blobs written with it.
- With the frame checksum on, zstd refuses a frame decoded with the wrong
  base \[M\]. Without it, a wrong base decodes to wrong bytes \[M\], which
  `Engine::restore` refuses on the image's own XXH64 checksum
  (`SnapshotError::Corrupt`) \[S\]. Keep the checksum on so the failure is
  zstd's, before any restore.

Measurements and the rejected engine-side alternative:
docs/research/base-relative-snapshots.md.

#### Upgrading: drain old builds

The upgrade policy (owner decision U1): **a snapshot restores only on a
build with the build id that wrote it, and the engine never migrates
one.** The id names what an image's meaning rests on, not the source
([Compatibility](#compatibility-the-build-id)), so an upgrade that changes
no layout keeps the id, and its old images restore on the new build: no
drain. When the id changes, a host keeps the old build available until the
old build's parked snapshots have drained, and treats a cell that no
available build can resume like a crash (an unknown outcome the host
reconciles).

Host steps:

1. Store each image with its build id (bytes 10..42 of the image,
   `micro_monty::build_id()`, `mm_build_id`), as above.
2. On upgrade, compare the new build's id with the old one's. The same id
   needs nothing more. Otherwise start new sessions on the new build and
   keep the old build loaded or installed, keyed by build id.
3. Restore each stored image on the build its id names. When every image of
   the old build has completed, been cancelled or been given up, retire it.
4. An image whose build is no longer available (or that fails with
   `SnapshotError::Incompatible`) is a crashed cell: report it as such, do
   not retry it on another build and do not re-run its source.

Ways to keep the old build:

- **wasm** (`crates/mm-wasm-host`): one `Runtime` holds several guest
  modules keyed by build id. `Runtime::load` adds a module (with its own
  precompiled `.cwasm`) and makes it current for new sessions;
  `Runtime::restore` reads the image's build id and picks that module, or
  returns `Error::UnknownBuild(id)` when it is not loaded; `Runtime::unload`
  retires a drained build. An old build is just an old `.wasm` file
  (`crates/mm-wasm-host/tests/upgrade.rs` parks cells on one build, runs
  new sessions on another in the same runtime, and resumes the old
  snapshots with identical output \[M\]).
- **Native, in-process:** a process links one micro-monty, so keep the old
  host binary (or the old `mm-cell-worker`) running, or installed beside the
  new one, and route old images to it until they drain.
- **Native snapshots through the wasm guest of the same build.** Native and
  wasm builds of the same source and features have the same build id (it
  hashes no target-specific input; `tests/upgrade.rs` asserts the native
  facade's id equals the guest's, on both wasm targets \[M\], and
  `durability --cross-target` restores every own case across native and
  wasip1). So a native host can drain its old snapshots on the old build's
  `mm_wasm_guest.wasm` under `mm-wasm-host`, without keeping an old native
  binary. This only works if the host keeps that guest module for every
  build it ships: build it from the same revision and features (stdlib
  features are part of the id) and store it keyed by build id beside the
  snapshots. Memory accounting differs between 32- and 64-bit targets, so
  leave headroom under the limit (Cross-target restore, below).

### Compatibility: the build id

A snapshot carries a 32-byte **build id**: a hash of what the image's
meaning rests on, and nothing else. It covers the snapshot format version
and a layout epoch, the code of the snapshot codecs, the engine's declared
layouts (every `const` and `#[repr]` item of the engine crates), the
opcode, native, type and native-step tables, the prelude's and the enabled
stdlib modules' bytecode, the stdlib features, the value and hash
encodings, and a recorded hash of what `Session::define` generates and
compiles (docs/spec.md §9.3, "Compatibility"). It does not cover the
version or the code of other functions: two builds that differ only there
share an id, and each restores the other's snapshots. `Engine::restore`
accepts only snapshots whose id equals its own and returns
`SnapshotError::Incompatible { format, build_id }`, naming the image's
format version and build id, otherwise
(`micro_monty::image_build_id(&image)` reads the id without restoring;
`SNAPSHOT_FORMAT` is this build's format). Snapshots are never migrated,
and source is never re-run to rebuild state: a session reopens only on the
same engine identity, with the identity being what images depend on.

In practice:

- Pin the micro-monty revision and features of anything that must read
  your snapshots, and compare build ids before an upgrade: a fix that
  changes no layout keeps the id. Store `micro_monty::build_id()` beside
  each snapshot to explain an `Incompatible` before you try a restore.
- Stdlib features are part of the id. `mm` enables the four opt-in modules
  (`uuid`, `unicodedata`, `html.entities`, `ast`), so its id differs from a
  default build's. Cargo unifies features across a
  build, so a binary built together with `mm` gets mm's modules and mm's id.
- `mm version`, `mm-cell-worker --version` and the wasip1 `roundtrip` example
  print the id. Release notes list it for every shipped binary.
- Corruption, truncation and checksum failures are `SnapshotError::Corrupt {
  what }`, never a panic or undefined behaviour. An engine panic while
  restoring (a bug, with unwinding) is `SnapshotError::Internal { message }`:
  the image may be sound, so keep it.

### Cross-target restore

Nothing in the build id depends on the target. A snapshot written on one
target restores on another when the build id matches:

- The ids agree across x86_64 Linux, aarch64 Linux (under qemu) and
  wasm32-wasip1, both for `mm` and for default-feature builds \[M\].
- Every `conformance/own` case was parked every few fuel units, each park
  saved by one `mm` process and continued by the other target's. Both
  directions passed, native→wasm32-wasip1 first and wasm32-wasip1→native
  first: every own case each way, output identical to an unparked run
  \[M\] (226 cases, 452 runs and 11,254 cross-target restores when last
  counted, before 2b3976c).
  `cargo xtask check` runs this as `cargo xtask durability --cross-target`
  (about 12 s).
- The format is explicit little-endian with canonical NaN-boxed values, so
  endianness does not matter. No big-endian host has run the gates; under
  Miri on `s390x-unknown-linux-gnu` a session that runs the same cell
  writes a byte-identical image to x86_64's, and each target restores the
  other's \[M\].

Caveats:

- Memory accounting charges native struct sizes, which differ between 32-
  and 64-bit targets. A session near its memory limit on wasm32 can be
  `TooLarge` on a 64-bit target with the same limit; leave headroom.
- Text and host-argument limits written on a 64-bit target clamp to the
  32-bit range on wasm32.
- Give the instance about 3 × the session's memory limit + 16 MiB of
  linear memory ([the Rust-heap bound](#embedding-guarantees)): a guest
  that runs out of linear memory aborts instead of raising `MemoryError`.
- On `wasm32-unknown-unknown`, set the wall clock and entropy again after
  restoring (std has neither there). Images restore across native, wasip1
  and unknown-unknown through the mm-wasm guest in every direction
  (`bench/evidence/isolation` `cross`, `crates/mm-wasm-host` tests) \[M\].
- Guest counts, lengths, widths and precisions are 64-bit on every target
  (`sys.maxsize` is `2**63 - 1` on wasm32 too): `len(range(2**40))`,
  `b'ab'.center(2**62)` (a `MemoryError`), `format(s, '.4294967296')` and
  `struct.calcsize('8589934592x')` give the same results on wasm32 as
  natively (conformance/own/`*_wasm32_counts.py`). Only the memory limit
  differs, as above.

## Optional stdlib modules

The default build bundles 55 modules: `json`, `re`, `collections`,
`itertools`, `functools`, `datetime`, `random`, `asyncio`, `typing`,
`base64`, `hashlib`, `dataclasses`, `operator`, `copy`, `heapq`, `bisect`,
`string`, `textwrap`, `enum`, `abc`, `binascii`, `time`, `os`, `pathlib`,
`fnmatch`, `posixpath`, `csv`, `io`, `gc`, `types`, `contextlib`,
`traceback`, `logging`, `urllib.parse`, `pprint`, `shlex`, `difflib`,
`struct`, `html` (with `html.parser`), `warnings`, `codecs`, `platform`,
`tempfile`, `decimal`, `numbers`, `contextvars`, `fractions`,
`statistics`, `tomllib`, `calendar`, `hmac`, `zlib` (checksums only),
`zoneinfo`, `tokenize` and `token`, plus `sys`, `math`, `cmath` and the
builtins, which are always built (one default build of about 2 MB, owner
decision S1). `uuid`, `unicodedata`, `html.entities` and
`ast` are opt-in features of `micro-monty`, outside the default set;
without one, importing it raises `ModuleNotFoundError: No module named 'ast'.
'ast' is not enabled in this build of micro-monty`. Without
`html.entities` (`mod-html-entities`), `html.unescape` resolves numeric
references and `&amp;`, `&lt;`, `&gt;`, `&quot;` and `&apos;` (with
CPython's no-semicolon forms), and leaves other named references as
written. Every bundled module has a `mod-*` feature on `micro-monty`
(dots and underscores become `-`: `mod-html-entities`; `urllib.parse` is
`mod-urllib`, and `token` and `html.parser` come with `mod-tokenize` and
`mod-html`); `sys`, `math` and
`cmath` have none, as they are always built. To enable one, list it:

```toml
micro-monty = { version = "0.0.1", features = ["mod-uuid"] }
```

`ast` (`mod-ast`) is CPython's `ast` over micro-monty's own parser:
`parse` (`exec`, `eval`, `single`), `dump`, `walk`, `NodeVisitor`,
`NodeTransformer`, `literal_eval`, `get_docstring`, `get_source_segment`
and the location helpers, with no `unparse` and no `compile()` of a tree.
`ast.parse` takes at most 32 KiB of source and `tokenize` (a default
module) 64 KiB, each in one native step; longer sources raise
`MemoryError` (`ast.parse() source larger than 32 KiB`).
It adds about 48 KB to `hello.wasm` \[M\], most of it the native node
builder, so it is opt-in:

```toml
micro-monty = { version = "0.0.1", features = ["mod-ast"] }
```

To leave default modules out, turn off `micro-monty`'s default features
and list the modules to keep; a module brings the modules it imports
when it is imported (`mod-datetime` brings `time`, `re`, `warnings` and
`operator`). A few functions import a module only when called and need
it listed too: `time.strptime` needs `mod-datetime`,
`Fraction.from_decimal` `mod-decimal`, `os.path.expandvars` `mod-re`, and
`types.coroutine` on a plain function `mod-functools`. Without `mod-asyncio`
(about 51 KB of `hello.wasm` \[M\]) `import asyncio` raises
`ModuleNotFoundError`, while `async def`, `await`, async generators,
`async with` and awaited host functions still work. Without `mod-json`
(about 31 KB) `import json` raises `ModuleNotFoundError`:

```toml
micro-monty = { version = "0.0.1", default-features = false, features = ["mod-re", "mod-collections"] }
```

### Smaller module sets for code-mode agents

Two lists measured against how models use the stdlib
([stdlib profile study](https://github.com/N0xMare/micro-monty/blob/main/docs/research/stdlib-profile.md)): 2,841
cells from real code-mode sessions (code review, `conformance/codemode/real`) and
1,782 one-shot solutions to API and data tasks (`conformance/codemode`).
No feature names them; copy the one that fits. Sizes are of the shipped
builds that `cargo xtask size` measures \[M\], at 3720bf2. The default
build was then hello.wasm 2,083,018 bytes, native `hello` 2,266,344 and the
mm-wasm guest 2,167,274; at 5df122e it is 2,092,164, 2,275,864 and
2,177,852 \[M\].

**Code mode** (34 features): every module that at least 2% of either
corpus imports, every module a real cell that succeeds needs, and
`textwrap`, `hmac` and `zlib`. It saves 85,495 bytes of hello.wasm
(4.1%), 89,488 of native `hello` and 85,260 of the guest. No
real cell that succeeds on the default build fails. 42 of the 1,782
synthetic solutions (2.4%) fail on an import: `zoneinfo` 21, `calendar`
9, `tomllib` 9, `shlex` 3. It leaves out `calendar`, `codecs`,
`contextlib`, `copy`, `dataclasses`, `difflib`, `enum`, `gc`, `logging`,
`platform`, `posixpath`, `pprint`, `shlex`, `string`, `tempfile`,
`tokenize`, `tomllib`, `traceback`, `types` and `zoneinfo`.

```toml
micro-monty = { version = "0.0.1", default-features = false, features = [
    "mod-json", "mod-re", "mod-collections", "mod-itertools", "mod-functools",
    "mod-datetime", "mod-random", "mod-asyncio", "mod-typing", "mod-base64",
    "mod-hashlib", "mod-operator", "mod-heapq", "mod-bisect", "mod-textwrap",
    "mod-abc", "mod-binascii", "mod-time", "mod-os", "mod-pathlib",
    "mod-fnmatch", "mod-csv", "mod-io", "mod-urllib", "mod-struct", "mod-html",
    "mod-warnings", "mod-decimal", "mod-numbers", "mod-contextvars",
    "mod-fractions", "mod-statistics", "mod-hmac", "mod-zlib",
] }
```

**Code review** (19 features, `mod-abc` comes with
`mod-collections`): the smallest set that every recorded real cell still
succeeds on; removing any module from it fails at least one. It saves
211,187 bytes of hello.wasm (10.1%), 221,280 of native `hello` and
210,812 of the guest, but 549 of the 1,782 synthetic solutions (31%)
fail on an import, mostly `datetime` 182, `csv` 103, `statistics` 64,
`html` 62, `urllib.parse` 47 and `decimal` 45. Use it only for agents
that read and search code.

```toml
micro-monty = { version = "0.0.1", default-features = false, features = [
    "mod-json", "mod-re", "mod-collections", "mod-itertools", "mod-functools",
    "mod-random", "mod-asyncio", "mod-typing", "mod-base64", "mod-hashlib",
    "mod-operator", "mod-heapq", "mod-bisect", "mod-binascii", "mod-os",
    "mod-pathlib", "mod-fnmatch", "mod-io", "mod-struct",
] }
```

Leaving modules out changes no speed: with the code-mode list, bench
and import instruction counts are within 0.02% and 0.7% of the default
build's \[M\].

Every module feature changes the build id. Depend on `micro-monty` only:
`mm-core`, `mm-compile`, `mm-parse`, `mm-engine` and `mm-stdlib` are its
internal crates, published for it alone, with no semver guarantees.

## Limitations

micro-monty aims at the Python that agents write, with output equal to
CPython 3.14 wherever a feature is supported. 546 of the 568 CPython-scored
cases of Monty's corpus pass \[M\] (`conformance/results/micro-monty.md`).
Unsupported syntax fails at compile time with "micro-monty does not support
X".

### Left out by design (spec §1, §4)

- **Ambient I/O.** No filesystem, network, subprocess, clock or randomness
  except through host functions. `tempfile` can be imported, but every
  function and class in it that would create or find a file raises
  `PermissionError` ("this sandbox has no ambient filesystem"), as `open()`
  does without a host `open`; only `gettempprefix()` and the constants
  work. `threading`, `socket`, `subprocess`,
  `multiprocessing`, `ctypes` and `sqlite3` do not exist.
- **Threads and `weakref`.** Both would make scheduling or GC timing
  observable. `async` is the concurrency model.
- **`gc` controls.** `gc.disable()` and `gc.enable()` only set the flag
  `gc.isenabled()` reports: collections still run, since a guest that
  turned them off could hold garbage up to the memory limit.
  `gc.collect()` waits for a collection that starts after the call (in
  steps over 32 MiB live, so it can park) and returns 0.
- **`__del__` and `__slots__`.** Rejected at compile time, because finalizer
  timing is not part of the contract. `__slots__` is deliberately out of
  scope: none of the 594 codemode census solutions uses it, nor any of
  the 1,782 after the GPT census (`conformance/codemode`) \[M\], and objects that refuse new
  attributes (CPython's `datetime`, slotted classes) accept them here.
- **A user `__getattribute__`.** A class body that defines one is rejected
  at compile time ("micro-monty does not support __getattribute__"):
  calling it would put a check on every attribute lookup's fast path.
  `__getattr__`, properties and descriptors work. A `__getattribute__`
  added to a class later (assignment, or in the namespace given to
  `type()`) is ignored, not called.
- **`exec`, `eval` and `compile`** are not defined
  (`NameError ... micro-monty does not support exec()`). Code arrives only as
  cells. `__import__(name, globals, locals, fromlist, level)` is CPython's
  over the same modules as the `import` statement (owner decision,
  2026-11-09; `conformance/own/425`). **`globals()`** is a read-only view of the module's globals
  (spec §4, decision G2): `'x' in globals()`, `globals()['x']`,
  `globals().get(...)` and iteration work as on CPython; any write raises
  `TypeError`.
- **Imports** are limited to bundled and host-registered modules.
  `sys.path` is an empty list that imports never search; there are no
  packages, no relative imports and no C extensions.
- **Host information.** `platform` reports fixed sandbox values on every
  host and target: `system()` is `'Linux'`, node, release, version and
  machine are empty, `python_implementation()` is `'micro-monty'`.
- **Surrogates.** A `str` is UTF-8, so it cannot hold a lone surrogate
  (U+D800 to U+DFFF), which CPython allows. A surrogate escape in a str or
  f-string literal (`'\ud800'`, `'\U0000dfff'`) is a compile-time
  `SyntaxError` ("micro-monty does not support surrogate escapes"), and
  `chr()`, `'%c'` and `format(n, 'c')` raise `ValueError` ("... arg is a
  surrogate ..."). A pair such as `'\ud83d\ude00'` is two lone
  surrogates in CPython too; write the character, or `'\U0001f600'`.
  Decoding never makes one: `errors='surrogateescape'` raises
  `NotImplementedError` ("micro-monty does not support lone surrogates
  (surrogateescape)") at the first undecodable byte, and
  `errors='surrogatepass'` decodes as `'strict'`, so an encoded surrogate
  raises `UnicodeDecodeError`. Both are no-ops when encoding a `str`.
  `json.loads` decodes an escaped pair (`"\ud83d\ude00"`) to its one
  character, as CPython does, and raises `json.JSONDecodeError` (a
  `ValueError`; "Lone surrogate escape, which micro-monty strings cannot
  hold") at the escape of a lone one, where CPython returns it.
- **`unicodedata` combining runs.** `unicodedata.normalize` and
  `is_normalized` (opt-in `mod-unicodedata`) normalize a run of
  characters with no normalization boundary in one step: a starter and
  the combining marks after it, or a run of Hangul vowel or trailing
  jamo. Such a run is capped at 65,536 characters after decomposition;
  past it they raise `MemoryError` ("unicodedata: more than 65536
  characters with no normalization boundary"). CPython has no such
  limit; real text stays far below it, as UAX #15's Stream-Safe Text
  Format allows at most 30 non-starters in a row.
- **t-strings** (PEP 750) are rejected. Annotations are parsed and never
  evaluated, as with deferred annotations.
- **No type checker** in the engine. Host functions validate their own
  arguments and raise CPython-style `TypeError`s.
- **No replay and no cross-version resume** (see Snapshots).

Not yet supported, with parity planned: PEP 695 `type` aliases (a compile
error); evaluated annotations: an annotation is kept as its source text
(`C.__annotations__` is `{'x': 'int'}`, a string annotation keeps its
quotes, functions have no `__annotations__`, and `typing.get_type_hints`
and `dataclasses.Field.type` give strings, so `functools.singledispatch`
cannot register from an annotation); `locals()`, and `vars()` without an
argument. The last two are planned for 0.0.2. `except*` follows CPython 3.14 (splitting, the rest re-raised, the
`break`/`continue`/`return` SyntaxError). `asyncio` follows CPython 3.14 for `run`,
`gather`, `sleep`, `create_task` (with `name=`), `wait_for`, `wait`
(`FIRST_COMPLETED`, `FIRST_EXCEPTION`, `ALL_COMPLETED`, `timeout=`),
`as_completed`, `shield`, `TaskGroup`, `timeout`/`timeout_at` (with
`reschedule()`, `when()` and `expired()`), `Lock`, `Event`, `Condition`,
`Semaphore`, `BoundedSemaphore`, `Barrier` (with `BrokenBarrierError`,
`abort()`, `reset()`), `Queue`, `LifoQueue`, `PriorityQueue`
(with `QueueEmpty`, `QueueFull`, `QueueShutDown` and `shutdown()`),
`Runner` (`run`, `close`, `get_loop`, context manager),
`current_task`, `all_tasks`, `ensure_future`, `isfuture`, and
`get_running_loop`/`get_event_loop`: the one running loop has `time()`,
`create_task`, `create_future`, `call_soon`, `call_later`, `call_at`
(cancellable `Handle`s), `call_exception_handler`, `is_running`;
`new_event_loop()`/`set_event_loop()` give loop objects over the same
engine loop whose `run_until_complete()` is a whole `asyncio.run()` (the
tasks it leaves are cancelled when it returns, where CPython keeps them).
The submodules `asyncio.tasks`, `taskgroups`, `locks`, `queues`,
`timeouts`, `exceptions`, `events`, `futures` and `runners` import and
hold the same objects. Each task runs in a copy of its creator's
`contextvars` context, or the one given to `create_task(context=)`.
`ExceptionGroup` and `BaseExceptionGroup` are builtins (`split`,
`subgroup`, `derive`) and are reported with CPython's nested boxes. Tasks
have `get_name`/`set_name`, `get_coro`, `get_loop`,
`add_done_callback`/`remove_done_callback`, `cancelling`/`uncancel`;
futures also have `set_result`/`set_exception`. Waiters are woken in
CPython's FIFO order, and a waiter cancelled after it was woken passes the
wakeup on, as CPython's do. `asyncio.Future` is a function, not a type.
Event-loop policies, threads (`to_thread`,
`run_in_executor`), subprocesses, streams and sockets are not available;
they raise with the `Processes` or `Network` hint. Eager task factories
are missing. "Task/Future exception was never retrieved" is reported when
`asyncio.run` ends (CPython reports it when the object is collected), and
a `KeyboardInterrupt` or `SystemExit` raised in a task leaves
`asyncio.run` after the main task is cancelled, as on CPython. Cancellation follows
CPython 3.14: `cancel()` passes on to what a task
awaits (another task, a gather's children, a future, a sleep, a host
future, an `__await__` iterator), `cancelling()`/`uncancel()` count the
requests, a cancel of the running task or of one that never ran raises at
its next step, and `asyncio.run` cancels the tasks still pending when its
coroutine ends and runs them until their cancellation is done. A cancelled
host future's call is `Abandoned` in the call log at once, and a later
answer to it is dropped. What still differs: tasks left at the end of
`asyncio.run`, and the other tasks of a failing `TaskGroup`, are cancelled
in creation order (CPython's order follows a set); functions have no
`__code__`, so task reprs and the "unhandled exception during
asyncio.run() shutdown" and "Exception in callback" reports have no
`coro=<...>` or source locations, and reports show no asyncio-internal
traceback frames; `loop.time()` and `TimerHandle` deadlines are virtual
time.
Time in
`wait_for` and `as_completed(timeout=)` is the virtual clock, and host calls
take none of it: a deadline waits for pending host calls, so it fires only
on `asyncio.sleep`s; time out slow host calls on the host side.
`async for`, `async with`, async comprehensions
and async generators work; as with generators, an async generator left
unfinished is never finalized (`asyncio.run` does not `aclose()` it).
Use `contextlib.aclosing` or explicitly await `aclose()` for cleanup.
Coroutine objects have `send()`, `throw()`, `close()` and `__await__()`,
user classes may define `__await__`, and `types.coroutine` marks a
generator function awaitable. When an `__await__()` iterator (or a
`types.coroutine` generator) yields, the coroutines awaiting it are
suspended and `send()` returns the yielded value, as on CPython; a
suspended coroutine survives later cells and snapshots. Under `asyncio`,
such a bare `yield None` yields to the event loop and any other value
raises `RuntimeError` ("Task got bad yield"). An `await` on an unsettled
asyncio awaitable (a host future, a task, `asyncio.sleep`) inside a
coroutine driven by `send()` still suspends the calling task instead of
yielding to the caller of `send()`. Coroutines have no `cr_frame`,
`cr_await` or `cr_code`. An unawaited coroutine gives CPython's
`RuntimeWarning` when the cell ends, not when it is collected (see
[Messages for models](#messages-for-models)).
`anext(it, default)` returns a coroutine rather than CPython's
`anext_awaitable`; `gather` accepts coroutines, not `asend()` awaitables
directly (await those inside a coroutine).
`inspect` is not bundled. `bytearray` works (constructors, item and slice
assignment, the list-like mutators, `+=`/`*=`, the bytes methods returning
bytearrays, comparisons with `bytes`; `conformance/own/340_bytearray.py`),
and `struct.pack_into` writes into one. Not supported: `memoryview`
(deferred),
subclasses of `bytearray`, `bytearray % args`, bytearray host values, and
bytearray arguments to `hashlib`, `re` and `json`, which take `bytes`
(spec §7.4). Parts of bundled modules are left out:
`logging.handlers`/`logging.config`
and file handlers, and `difflib.HtmlDiff`. `zlib` has only `crc32` and
`adler32`: there is no deflate, so `zlib.compress`, `decompress`,
`compressobj` and `decompressobj` raise `AttributeError` (the message says
only the checksums are bundled); `gzip` and `zipfile` do not exist.
`calendar` knows only the C locale (`LocaleTextCalendar` with another
locale raises `locale.Error`) and has no command line. `hmac.compare_digest`
is not constant-time. `zoneinfo` loads zones only through the host's zone
source (below); `available_timezones()` raises `NotImplementedError`, and
`TZPATH` is empty. Log records carry no caller
location (no frame objects). `codecs` has no incremental or stream codecs;
codecs and error handlers registered with `codecs.register` and
`codecs.register_error` are seen by `codecs.lookup`, `encode`, `decode`
and `lookup_error`, not by `str.encode` or `bytes.decode`. The codec
errors the engine raises carry only their message, so the `codecs` error
handlers accept only `UnicodeEncodeError`/`UnicodeDecodeError` built with
all five arguments.

Metaclasses other than `type` work (`__new__`, `__init__` and `__call__`
overrides, class keywords, metaclass methods). A metaclass with
`__prepare__` raises `NotImplementedError` ("micro-monty does not support
__prepare__") when the class is created. Wildcard imports work at module
level. All three are checked against CPython in `conformance/own/311_*`,
`312_*` and `313_*`.

`dir()` follows CPython (`conformance/own/330_dir.py`): with no argument
it lists the module globals, a function's bound locals and free variables
or a class body's namespace (inside a comprehension, the enclosing
scope's, without the comprehension's own variables); a `__dir__` on the
object's class or metaclass (and `object.__dir__`, `type.__dir__` and
`module.__dir__`, also through `super()`) is honoured, all in prelude
Python. Builtin types list their
method table plus a fixed set of CPython's slot and data-descriptor names,
so `'append' in dir([])` and `'__add__' in dir(int)` hold, but the lists
are not CPython's exactly: methods micro-monty lacks
(`float.from_number`) are missing, and some listed dunders (`list.__add__`) are VM
operations that `getattr` does not return. The module globals have only
`__name__` (and `__file__`) as dunders, classes have no
`__firstlineno__`/`__static_attributes__`, `dir()` takes no keyword
argument but its own, and a global named `dir` that is not the builtin is
called with that hidden `<scope>` keyword from inside functions.
`type.__dict__` for a builtin type is a fresh dict of its method table, not
a read-only mapping proxy.

`sorted` and `list.sort` are a stable merge sort that calls only `<`, as
CPython's sort does, not CPython's timsort. For a consistent total order
the result is the same, but the comparisons differ: user `__lt__` methods
are called on other pairs and a different number of times
(`sorted([K(2), K(1), K(3)])` calls `__lt__` 4 times on CPython and 3 times
on micro-monty), a mixed-type list can fail on another pair first
(`sorted([0, 0, 'a', 0])` reports `'int' and 'str'` where CPython reports
`'str' and 'int'`), and orders that are not total (sets, objects whose
`__lt__` is inconsistent) can sort differently.

`sum` over a generator or another iterator that runs Python code checks
each running int total with `>> 63` to see that it still fits 64 bits, the
bound of CPython's exact int stage: if an item's `__radd__` returns an
object whose class defines `__rshift__`, that method is called with 63
(CPython calls none), and an exception it raises other than `TypeError`
ends the `sum`.

Other comparison differences. Floats are NaN-boxed with one canonical NaN,
so two distinct NaN objects are the same object: `[x] == [y]`, `{1: x} ==
{1: y}` and `{x} == {y}` are `True` for `x = float('nan')` and `y =
float('nan')` (CPython: `False`). Comparing nested containers raises
`RecursionError` past 1,000 levels whatever `Limits::recursion_depth` is
(the walk's `MAX_NEST`); CPython compares 1,100-deep lists.

Small numeric and hashing differences: `math.cbrt` uses FreeBSD msun's
algorithm, so its last digit can differ from CPython's on glibc
(`cbrt(27)` is `3.0`, glibc's `3.0000000000000004`): glibc's code is
copyleft and is not ported (`libm.rs`). `UnicodeEncodeError` and
`UnicodeDecodeError` have no `encoding`,
`object`, `start`, `end` or `reason` attributes, and one built with all
its arguments prints them as a tuple. `collections.Counter` counting an
iterable hashes each element twice (CPython's C helper once), which shows
only in a `__hash__` with side effects.

Iterators and ranges. `list.extend` and list `+=` read their whole
iterable before the list grows, so a generator that reads the list sees
it unchanged: `a += g()`, where `g` yields `len(a)`, gives `[1, 1, 1, 1]`
where CPython gives `[1, 1, 2, 3]`. `any`, `all`, `list`, `tuple` and
`sorted` behave as on CPython. `range` start, stop and step must fit in a
signed 64-bit integer: `range(2**63)` and `range(0, 2**70, 2**68)` raise OverflowError
when built, where CPython takes any int and raises only from `len()` of a
range too long for it. A `dict` subclass's
`__missing__` is called by `d[k]` but not through `d.__getitem__(k)` or
`dict.__getitem__(d, k)`, which raise `KeyError`.

`json.loads` with hooks parses the whole document before calling any hook,
so on malformed input no hook has run when `JSONDecodeError` is raised
(CPython calls them as it goes), and nesting deeper than CPython's
recursion limit parses instead of raising `RecursionError`. `help()` prints
a short stub (`Help on function f:` and the docstring, without the
signature or module) and with no argument prints one sentence instead of
starting interactive help. `globals()` is not a `dict`
(`isinstance(globals(), dict)` is `False`).

### Other differences from CPython

Each checked against CPython 3.14.7 at eb78933 \[M\]:

- **Missing names.** The builtins `input`, `breakpoint`, `exit`, `quit`,
  `copyright`, `credits` and `license`; the exceptions `BlockingIOError`,
  `BrokenPipeError`, `ChildProcessError`, `ConnectionError` and its
  subclasses, `InterruptedError`, `ProcessLookupError`,
  `PythonFinalizationError` and `UnicodeTranslateError`; the module
  `builtins`; `__builtins__`, `__spec__` and a module's `__doc__`;
  `sys.modules`, `sys.getsizeof`, `sys.intern` and `sys.getrefcount`;
  `function.__globals__`; `sys.stdout.writelines`; `traceback`'s
  stack-walking functions (`extract_stack`, `format_stack`, `walk_stack`);
  `string.Formatter`; `re.Scanner`; `functools.Placeholder`;
  `math.nextafter(..., steps=)`; the dunders of builtin types as type
  attributes (`dict.__or__`, `str.__mod__`); `dict_keys.mapping`; and
  `__length_hint__` on list and set iterators.
- **Subclassing.** `frozenset`, `complex`, `property`, `classmethod`,
  `staticmethod`, `zip`, `enumerate` and `super` cannot be subclassed
  (`TypeError: micro-monty does not support subclassing ...`), and `map`,
  `filter` and `reversed` are functions, not types.
- **Codecs.** UTF-8 (and `utf-8-sig`), UTF-16, UTF-32, ASCII and Latin-1.
  Others (`cp1252`, `utf-7`, `unicode-escape`, `idna`, and `rot13`, `hex`
  and `base64` through `codecs.encode`) raise `LookupError`.
- **Literals and SyntaxError text.** Named Unicode escapes (`'\N{BULLET}'`)
  are a `SyntaxError`, and `\N{...}` in an `re` pattern raises
  `re.PatternError`; write the character or `'\u2022'`. SyntaxError text
  follows CPython 3.14 except that a misspelt keyword gets no "Did you
  mean", two expressions side by side in a dict value or subscript get no
  "Perhaps you forgot a comma?", an f-string open at the end of the source
  reports `f-string: expecting '}'`, and a line-continuation error's caret
  sits on the backslash.
- **`ast` and `tokenize`** (opt-in and default modules over the own
  parser and lexer). `ast.parse` refuses sources over 32 KiB and
  `tokenize` over 64 KiB with `MemoryError`, so that one native step stays
  short. `ast.parse` in `eval` mode reports `print "x"` as plain invalid
  syntax, and some error end offsets differ. `tokenize` has no
  `untokenize` or `open`, raises `TokenError` on `0777` where CPython
  yields a NUMBER, and starts a new row after a lone `\r`.
- **asyncio.** `asyncio.create_task` outside a running loop does not raise
  `RuntimeError` ("no running event loop"); the task runs in the next
  `asyncio.run`. `loop.run_forever()` and `loop.stop()` raise
  `RuntimeError`. A `KeyboardInterrupt` or `SystemExit` raised in a task
  is not reported as "Task exception was never retrieved".
- **Numbers.** Besides `math.cbrt`, `exp`, `log`, `log2`, the
  trigonometric and hyperbolic functions, `erf`, `erfc`, `gamma`, `atan2`
  and float `**` can differ from glibc's results in the last digit (108 of
  92,000 random calls).
- **Objects.** One object holds at most 64 MiB whatever the memory
  limit ([Limits](#limits)). `namedtuple` and dataclasses take at most 126 fields
  ("micro-monty does not support more than 126 parameters here"); a plain
  `def` takes more.
- **Memory near the limit, unparked against restored.** Near the memory
  limit, an operation the engine does not run again after a collection
  can raise `MemoryError` in an unparked run and succeed after a snapshot
  and restore at a park just before it, because a snapshot collects
  garbage first. Such operations include a dict's or set's in-place `|=`,
  a builtin that is not a method of an immutable type, and an instruction
  right after one that drew a one-shot iterator with no call or loop jump
  between. Operations that run again after a collection (spec §9.2) give
  the same outcome either way. For the same reason, a snapshot taken
  inside an out-of-memory retry wait, restored or not, can change the
  fuel total, once per wait: `f(*big, **kw)` near a 256 MiB limit, with
  a snapshot at every 50,003-unit park, costs 28,672 units (0.64%) more
  than unparked \[M\]. To be narrowed in 0.0.2.
- **Fuel after a restore, long non-ASCII strs.** The char-position
  cache that makes indexing and searching a long non-ASCII str cheap is
  not in a snapshot, so the first walk after a restore is charged again.
  A single `find` or `index` totals the same either way, and a mixed
  program costs 1.7% more; but an index loop restored at every park
  pays the walk at each one: `for k in range(1, 1_600_000, 997): W[k]`
  costs 43,313 units unparked and 8,687,009 with a restore at every
  20,011-unit park \[M\]. Hosts that restore at every park should avoid
  that shape or use larger slices.
- **Smaller ones.** `io.StringIO.seek` accepts a float. `bytes.hex(None)`
  is accepted, `bytes.fromhex` does not treat a vertical tab as
  whitespace, and `b''.decode()` with an unknown codec raises
  `LookupError` (CPython returns `''`). An exception whose only argument is
  itself has the `repr` `Exception(...)` and a `str` that raises
  `RecursionError`. A set display of constants whose hashes collide can
  iterate in another order than CPython's, which builds it from a
  frozenset constant (`set([...])` matches). A `string.Template` subclass's
  `delimiter` and `idpattern` are ignored. `S % ()` and `S.format()` on a
  `str` subclass return a plain `str`. A dataclass's
  `__doc__` is `None`, an `Enum` member may be named `mro`, a
  `functools.partial` subclass's repr says `functools.partial`, and
  `hmac.compare_digest` stored as a class attribute binds as a method.
  `re`'s ignore-case matching misses a few case folds (`ﬅ`/`ﬆ`, the Kelvin
  sign in a class range). `dict([1])`'s message names a sequence where
  CPython's says the element is not iterable.

### Code size limits

Source CPython compiles can fail to compile on micro-monty past these
limits, always with a `CompileError`, never a crash:

- **Nesting, 200 levels** (`RecursionError: maximum recursion depth
  exceeded during compilation`). Each bracket, call, conditional arm,
  unary operator, `**` and block counts one, a lambda two (99 nested
  lambdas compile). CPython limits brackets to
  200 too (`too many nested parentheses`), but nests `not`, `-` and
  `lambda` thousands deep. A chain is one level whatever its length:
  operators (`a + b - c * d`), attribute, subscript and call chains
  (`x.f(1)[0].g()`), conditional expressions chained in their `else`
  (`a if p else b if q else c`), `elif` arms, f-string fields and string
  concatenation. Up to 100,000 chain operators per expression are accepted;
  CPython's compiler runs out of stack between 12,000 and 50,000 `+` terms.
- **255 registers per function** (`SyntaxError: micro-monty does not
  support expressions needing more than 255 registers`). Every value an
  expression holds while computing another takes one: an assignment
  unpacking into more than about 250 targets, more than about 250
  decorators on one definition, or nested calls more than 85 deep
  (`f(g(h(...)))`) and nested dicts more than 127 deep, which reach it
  before the nesting limit. Lists, tuples, dicts, sets, call arguments
  and f-strings of any length do not.
- **250 local variables per function** (`SyntaxError: micro-monty does not
  support functions with more than 250 local variables`). Module and class
  bodies are not limited.

### Compared with Monty

Monty (pydantic's interpreter, the donor of the conformance corpus) makes
different choices [S: <https://pydantic.dev/docs/monty/limitations/>,
2026-11-04]:

| | Monty | micro-monty |
|---|---|---|
| Classes | simple classes; inheritance, metaclasses and method decorators (`@property`, `@classmethod`, `@staticmethod`) rejected | inheritance with C3 MRO, `super()`, `property`/`classmethod`/`staticmethod`, the core dunder set incl. `__bool__`, `__len__`, `__lt__`, `__getattr__` |
| Exceptions | fixed built-in set; no user exception classes | user exception subclasses |
| Generators | no `yield`; generator expressions materialize a list | `yield`, `yield from`, `send`/`throw`/`close`; lazy generator expressions |
| `del`, function attributes | rejected / `AttributeError` | supported |
| `map`, `zip`, `filter`, ... | eager | lazy, as in CPython |
| `eval` / `exec` | source text only | not available |
| `assert` | pytest-style messages by default | CPython's bare `AssertionError` |
| `re` | Rust `fancy-regex`: no `bytes` patterns, no `VERBOSE` | own engine with `bytes` patterns and `VERBOSE` |
| Codecs | UTF-8/16/32 and ASCII only | also `latin-1` |
| Wildcard imports | rejected | accepted |
| `async with`, `async for`, async generators | rejected | supported |
| `except*` | rejected | supported, as CPython 3.14 |
| `type` aliases, t-strings | rejected | rejected |
| Missing modules | requested from the host | `ModuleNotFoundError` unless defined with `Session::define` |
| Host data | host class (dataclass) instances in and out | plain data, and host classes as immutable values (`HostValue::Object`) |
| Type checking | built in, static, against stubs | at the call, for typed host functions; `stubs()` for an external checker |
| Stopping a running cell | only while waiting on the host | any safepoint, any depth (pause-anywhere) |
| Snapshots | serialized heap | FULL image, same build id, validated on restore |

## Typed host functions

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default());
# let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?;
# let budget = 1_000_000_u64;
# let _ = (engine, code, budget);
# let _ = {
use micro_monty::{HostModule, HostParam, HostSignature, HostValue, ParamKind};

session.define(&HostModule {
    name: "fs".into(),
    functions: vec![HostSignature {
        name: "read".into(),
        params: vec![
            HostParam { name: "path".into(), annotation: "str".into(), default: None,
                        kind: ParamKind::PositionalOrKeyword },
            HostParam { name: "limit".into(), annotation: "int | None".into(),
                        default: Some(HostValue::None), kind: ParamKind::KeywordOnly },
        ],
        returns: "str".into(),
        doc: "Read a text file.".into(),
        ..HostSignature::default()                 // dict_as_kwargs: false
    }],
    ..HostModule::default()
})?;
# }; Ok(()) }
```

A typed host function binds its arguments as a Python `def` with that
signature would. Missing, extra, duplicate and unknown arguments raise
CPython's `TypeError` messages at the call. Each parameter whose annotation
the boundary can check is then tested:

- **Checked annotations:** every `|` part must be `None`, `str`, `bytes`,
  `bool`, `int`, `float` (accepts `int`), `list`/`tuple` (either), `dict` or a
  host class defined in this or another module. Subscripts like `list[str]`
  check only the outer type.
- **Failure:** a mismatch raises `TypeError: read() argument 'path' must be
  str, not int` where the call is. The host never sees such a call, and the
  traceback ends at the caller's line.
- **Unchecked annotations** (`Any`, `Literal[...]`, `Sequence[str]`, ...)
  only appear in the stubs.

The host then receives every parameter positionally, in declaration order,
with defaults filled and no `kwargs`. `fs.read('a')` arrives as
`HostCall { name: "fs.read", args: [Str("a"), None], kwargs: [], .. }`.
`*args` and `**kwargs` exist only in the untyped form `(*args, **kwargs)`.

There is no static check before the cell runs, and micro-monty will not add
one: a cell with a type error runs up to the bad call and raises `TypeError`
there. For "type errors; nothing ran", run a checker over `stubs()` on the
host side before starting the cell.

**One dict as the keyword arguments.** Set `dict_as_kwargs: true` on a typed
signature and a call with exactly one positional `dict` and nothing else
binds that dict's items as keyword arguments, then checks them as usual:
`fs.read({"path": "a", "limit": 3})` is `fs.read(path="a", limit=3)`, and
`fs.read({"path": 1})` raises `read() argument 'path' must be str, not
int`. This accepts both conventions models write: a single dict (as in
Anthropic's programmatic tool calling) and keyword arguments \[S\]. It is off by default
because it is ambiguous for a function whose only parameter is itself a
`dict`; with other arguments (`fs.read({...}, limit=3)`) the dict is the
first parameter as usual. On a method it applies after `self`.

**Introspection.** Host functions, typed or untyped, have `__name__`,
`__qualname__`, `__module__` (the host module, or `builtins`) and `__doc__`
(`HostSignature::doc`, else `None`). `dir(module)` lists a host module's
functions, classes, exceptions and constants, and `help(f)`, `help(Class)` or
`help(module)` prints the stub `stubs()` renders for it (its signature and
docstring). Search over many tools stays the host's: the engine answers
these lookups only.

## Host classes

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default());
# let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?;
# let budget = 1_000_000_u64;
# let _ = (engine, code, budget);
# let _ = {
use micro_monty::{HostClass, HostModule, HostSignature};

session.define(&HostModule {
    name: "files".into(),
    functions: vec![HostSignature::any("listdir")],
    classes: vec![HostClass {
        name: "Entry".into(),
        fields: vec![("path".into(), "str".into()), ("size".into(), "int".into())],
        methods: vec![HostSignature { name: "read".into(), returns: "bytes".into(),
                                      ..HostSignature::default() }],
        doc: "A directory entry.".into(),
    }],
    ..HostModule::default()
})?;
# }; Ok(()) }
```

A host class is an immutable value with named fields. In Python,
`files.Entry('a.txt', 3)` builds one, and fields read as attributes or by
index. Instances compare and hash by value, match `case Entry(path, size)`,
and refuse assignment. They are `tuple` subclasses, like named tuples.

An instance crosses the boundary in either direction as
`HostValue::Object { class: "files.Entry", fields: [("path", ..), ("size", ..)] }`,
with fields in the class's order. `class` is the module-qualified name, or the
bare name for a class of the builtins module. Methods are typed host
functions: `e.read()` reports
`HostCall { name: "files.Entry.read", args: [Object {..}, ...], .. }`. An
`Object` of an undefined class, or with other fields, raises `TypeError`
where the host answer arrives.

A class is a copy of host data, never a live reference, so it stays
snapshot-safe. A handle is a class whose fields hold an id.

## Stubs

`session.stubs()` returns `.pyi` text as `(module, text)` pairs, derived from
the same `HostModule` values. The first pair is `""`, for the builtins; then
comes one per host module, by name. Each lists its constants, classes and
functions in definition order.

- **Typed functions and classes** show their signatures and docstrings.
- **Untyped functions** take `*args: Any, **kwargs: Any` and return `Any`.
- **Constants** show their type.
- **The OS hooks** (dotted builtins such as `os.getenv`) are left out,
  because cells cannot name them.

Use them in the model's prompt, or with an external type checker.

```python
from typing import Any

ROOT: str
class Entry:
    """A directory entry."""
    path: str
    size: int
    def __init__(self, path: str, size: int) -> None: ...
    def read(self) -> bytes: ...
def listdir(*args: Any, **kwargs: Any) -> Any: ...
```

## Session templates

A template is a session saved after its setup (host definitions, a
bootstrap cell, common imports) and restored for each new session:

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default());
# let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?;
# let budget = 1_000_000_u64;
# let _ = (engine, code, budget);
# let _ = {
let mut setup = Session::new(&engine)?;
setup.define(&HostModule::untyped("fs", &["read"]))?;
// Load the modules, then delete the names: instances see clean globals.
setup.start(&engine.compile("import json, zoneinfo\ndel json, zoneinfo\n", "setup.py")?);
let template = SessionTemplate::capture(&engine, &mut setup)?;   // between cells
let mut session = template.instantiate()?;   // a validated restore, not a re-run
session.set_fuel(Some(budget));               // the first cell's budget; then the list below
# let probe = engine.compile("'json' in globals()", "probe.py")?;
# let Progress::Complete(Some(c)) = session.start(&probe) else { panic!() };
# assert_eq!(c.repr, "False");
# }; Ok(()) }
```

Instances are independent: a change in one is invisible to every other, and
templates are `Send + Sync`. `image()` and `from_image` store a template
and load it back in a process of a build with the same build id.

**Rules for the setup:**

- **Leave no names behind** (`del` after `import`). Otherwise a cell that
  forgets `import asyncio` works from the template and fails in a fresh
  session.
- **Reseed `random`** with a `random.seed()` cell after `instantiate()` if
  the setup loaded it, directly or through `statistics` or `uuid`.
  Otherwise every instance draws the same numbers.
- **Capture with the clock the instances use.** `time` (loaded by
  `datetime`) keeps import-time readings: `process_time()` counts from the
  capture, and an instance whose clock is earlier than the capture's sees
  `monotonic()` stand still.

**Not in the image**, as after any restore:

- the host-side settings (output, text cut, `int_bits_text`, GC stress,
  the wall clock, entropy and the zone source; zones loaded before the
  capture still work): pass them to `instantiate_with(settings)`, which
  takes the same `SessionSettings` as `Session::new_with`; `instantiate()`
  uses the defaults (platform clock and entropy, none on
  `wasm32-unknown-unknown`);
- the deadline, and `park_handle()` (take it again);
- a cell budget other than the engine's `Limits::fuel` (`set_fuel`); the
  image keeps what is left of the fuel total.

Kept in the image: module dunders, `set_last_expr_value`, `set_host_args`,
`set_text_limit`, `set_hint`, `set_local_timezone`, host definitions and
the limits (the memory limit becomes the smaller of the image's and the
restoring engine's).

**Invalidation.** A template image carries the build id like any snapshot,
and a build with another id refuses it (`SnapshotError::Incompatible`;
`mm-wasm-host` picks the guest by the id). Rebuild templates from their
setup code whenever the build id changes instead of draining them (unlike
parked snapshots, "Upgrading: drain old builds"). Key them by build id and
setup code, not by their bytes: captures differ where imports read the
clock or entropy. A template written natively restores on both wasm targets of the
same build id.

**Large tool sets.** A template's image refers to its host modules' code
by key, like any snapshot, so `instantiate()` on the capturing Engine (or
its clones) takes the compiled code from the Engine's cache. With the
cache, defining the modules in each new session also reuses the compiled
code (0.9 ms for 100 typed functions \[M\]). A template therefore helps
large tool sets mostly through its imports and bootstrap cell, not through
the compile.

**When it pays.** A restore costs about 5–6 µs per KB of image natively
\[M\]. The table below was measured before the Engine's module cache, when a
restore also decoded every imported module's bytecode
(docs/research/session-templates.md) \[M\]:

| Setup | fresh session + imports | restore | image |
|---|---:|---:|---:|
| `json` | 169 µs | 59 µs | 52 KB |
| json, asyncio, re, collections, datetime | 4.9 ms | 2.6 ms | 517 KB |
| decimal, statistics, zoneinfo, html.parser, tomllib | 30.6 ms | 7.1 ms | 1.2 MB |

With the cache warm (one `Engine` per process), the codemode-set restore
is 1.1 ms and the heavy set 2.7 ms \[M\]
(bench/evidence/results/2026-11-07-module-cache). Use a template when most
cells import its modules, especially heavy ones (zoneinfo, decimal,
statistics, tomllib). A broad template made the
codemode suite's median cell slower (2.1 → 4.6 ms per cell), because most
cells import only `json`. Through `mm-wasm-host`, a restore costs about
0.6 ms more than a new session, so only large setups gain there. There is
no pool: instantiate on demand, or keep spares on your own executor.

## Remaining SDK gaps

- **`mm-cell-worker`** does not expose typed functions or host classes on
  the cell protocol yet; a host that wants them embeds the crate.
