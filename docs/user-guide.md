# micro-monty user guide

`micro-monty` is a Python 3.14 subset that runs inside your Rust process.
It has no ambient I/O: files, network, clocks and randomness reach Python only
through functions you provide. A running cell can stop at any safepoint, at
any call depth, and continue later: in the same process, on another
thread, or from a snapshot in another process or on another target.

This guide is the `micro-monty` crate's documentation, and its examples
are its doctests. It covers the Rust embedding API, the `mm` command line,
WebAssembly hosts, snapshots, the optional stdlib modules and the known
differences from CPython. The design is in the
[spec](https://github.com/N0xMare/micro-monty/blob/main/docs/spec.md).
Numbers are labelled \[M\] measured, \[S\] sourced or \[E\] estimated;
measurements are of 0.0.1 or the builds just before it, on x86_64 Linux
release builds, unless stated.

```toml
[dependencies]
micro-monty = "0.0.1"   # or { git = "https://github.com/N0xMare/micro-monty" }
```

Releases are `0.0.x`, and Cargo treats each one as incompatible with the
next: `"0.0.1"` never upgrades to 0.0.2 on its own. There are no
compatibility layers between releases: moving to a new one is a
deliberate change, and its snapshots follow
[Upgrading: drain old builds](#upgrading-drain-old-builds).

The crate is pure Rust: no C toolchain and no files beside it. The bundled
stdlib is compiled in. It builds for `wasm32-wasip1` and
`wasm32-unknown-unknown` too. Depend on `micro-monty` only: `mm-core`,
`mm-compile`, `mm-parse`, `mm-engine` and `mm-stdlib` are its internal
crates, published for it alone, with no semver guarantees.

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
[Driving a session](#driving-a-session) shows a shorter form.

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
  traceback, truncated })`, with the traceback in CPython 3.14's format.
  The host's own mistakes are `DefineError`, `ResumeError` (with the
  session's `Phase`) and `SnapshotError`. The error and result types are
  `#[non_exhaustive]`: match them with a wildcard arm, and build the plain
  structs a host constructs with `..Default::default()` (`HostError` with
  `..HostError::new(..)`).
- **Threads.** A session can be stopped on one thread and resumed on another,
  so every callback it holds must be `Send`. On tokio, move the session
  into `spawn_blocking` for each step, or give it fuel in slices so each step
  returns quickly. Share one `Engine` and its `Code` across tasks with `Arc`.
- **Many sessions.** Sessions share no mutable state, so one process can
  run many with independent limits. A new session is cheap: `mm run
  print(0)` costs 0.90M instructions for the whole process \[M\]. Sessions
  of one `Engine` (or its clones) share decoded stdlib bytecode, so keep one
  `Engine` for the process.
- **Panics.** Built with unwinding, the facade catches an engine panic and
  poisons only that session: every later call returns a `SystemError` and
  `is_poisoned()` is true (see [Embedding guarantees](#embedding-guarantees)).

## Limits

| Resource | Setting | When it runs out |
|---|---|---|
| Heap | `Limits::memory` (default 256 MiB): arena, retained code and sources, caches, frames, task stacks, host-module definitions | catchable `MemoryError`; big allocations (`*`, `pow`, `<<`, `join`, ...) are checked before they start |
| CPU | `Limits::fuel` per cell (default unlimited); `Session::set_fuel` for the next cell or a parked one; `Session::set_fuel_total` for all of a session's cells together. One unit per call or backward jump, plus proportional charges by long natives | parks: `Parked { reason: Fuel }` |
| Wall time | `Session::set_deadline(Some(instant))`, or `set_deadline_with(now, deadline)` with your own clock | parks: `Parked { reason: Deadline }` (one-shot; set it again before resuming) |
| Python stack | `Limits::recursion_depth` (default 1000) | `RecursionError` |
| Rust stack | running code keeps its nesting in the arena; the parser and compiler are depth-counted | `RecursionError`, never a stack overflow |
| Source | `MAX_SOURCE_BYTES` = 256 KiB; nesting and per-function limits in [Code size limits](#code-size-limits) | `CompileError` |
| Host arguments | `Session::set_host_args(HostArgs { max_bytes, max_depth, lenient })` (default 16 MiB, depth 100, strict) | an exception in Python before the host sees the call (`ValueError` past `max_bytes`, `RecursionError` past `max_depth`, `TypeError` for a value with no host form or a non-`str` dict key, `MemoryError` past the memory limit), or `HostValue::Unconverted` pieces when `lenient` |
| Completion text | `Session::set_text_limit(bytes)` (default 1 MiB, or the memory limit if less), `set_text_cut(TextCut)`; printed output is not cut | `repr`/`str` cut (see "Long results" under [Messages for models](#messages-for-models)); `value` stays whole |
| int ↔ str | CPython's 4300-digit limit | `ValueError`; `set_int_bits_text(true)` renders `<int with N bits>` in completions instead |
| One object | 64 MiB of a `str`'s or `bytes`' data, or of a list's, tuple's, dict's or set's slots, whatever `Limits::memory` is: at most 8,388,608 list items, 2,097,152 dict entries, 5,033,164 set items \[M\] | catchable `MemoryError` |
| `json.dumps` result | min(0.48 × `Limits::memory`, 64 MiB) of text \[M\]: the old and new buffers coexist while it grows. At a 16 MiB limit, about 7.7 MiB | catchable `MemoryError` |
| Starred displays, calls and f-strings | the largest result beside inputs as large as itself, as a fraction of `Limits::memory` (at most 64 MiB) \[M\]: `[*a, *b]` 0.35; `(*a, *b)`, unpacking and `f(*a, *b)` 0.25; an f-string 0.30 | catchable `MemoryError` |
| Int size and int work | 2^26 bits per int; a superlinear int operation (`*`, `//`, `%`, `divmod`, `**`, `pow(a, e, m)`, `math.factorial`/`comb`/`perm`/`isqrt`/`gcd`/`lcm`, ...) estimated to take more than about 45 ms; reducing an int of any size by a modulus of up to 128 bits (`%`, `//`, `divmod`, `pow(a, e, m)`'s base, `gcd`) is not refused | `MemoryError` before it starts (`integer operation too large` for the work cap), whatever the fuel |
| `ast.parse` / `tokenize` source | 32 KiB / 64 KiB | `MemoryError` |
| Snapshot output | `snapshot_with_limit(max_bytes)` (default: the memory limit) | `SnapshotError::TooLarge` |

Every byte count the API takes is a `u64` on every target; a limit larger
than the address space (`u64::MAX` included) means no limit, never a panic.
`Limits::memory` bounds what the session retains, not process RSS
([Embedding guarantees](#embedding-guarantees)). Fuel bounds work; bounded
native steps bound latency: a native operation charges fuel in proportion
to its work, and one whose work a guest can make large runs in steps of at
most 2^17 units, parking between them (spec §5.1). Large int operations and
long sorts park before starting when the fuel left cannot pay their
estimate. A forced `gc.collect()` is not charged to fuel, so fuel alone
does not bound CPU time (4,000 calls with 100,000 live lists: 5 s for
120,076 units \[M\]); set a deadline too.

**Fuel is per cell.** Each `start` gives the cell `Limits::fuel`, unless
`set_fuel` was called since the last cell (then the next cell gets that,
once). On a parked cell, `set_fuel` refuels it before `Continue`. With
`set_fuel_total(Some(n))`, each cell starts with the smaller of its budget
and what is left of `n`, and every unit charged under a fuel limit comes
off the total; once it is spent, every cell parks on fuel at its first
safepoint and `fuel_total()` is `Some(0)`. Refuelling a parked cell is the
host's decision and is not capped. Snapshots and templates keep what is
left of the total.

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default()); let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?; let budget = 1_000_000_u64; let _ = (engine, code, budget); let _ = {
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
is delivered synchronously as it is produced and is never cut, so the host
enforces its own caps; a code-mode worker typically caps a cell's output at
about 1 MiB. `micro_monty::head_tail(text, max_bytes)` cuts collected output
to its head and tail.

## Host functions and host modules

The host surface is plain data: a `HostModule` per module (`name`, where
`""` is the builtins and `"fs.meta"` a submodule; `functions`, `classes`,
`constants` and `exceptions`), defined with `Session::define`.

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default()); let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?; let budget = 1_000_000_u64; let _ = (engine, code, budget); let _ = {
let ids = session.define(&HostModule::untyped("", &["lookup"]))?;  // builtins; globals may shadow them
session.define(&HostModule {
    constants: vec![("ROOT".into(), HostValue::Str("/".into()))],
    ..HostModule::untyped("fs", &["read", "write"])
})?;
session.define(&HostModule::untyped("fs.meta", &["stat"]))?;      // also the attribute `fs.meta`
# }; Ok(()) }
```

- **Whole-module replacement.** Each `define` replaces everything under that
  name, so nothing from an earlier definition lingers.
  `HostModule::untyped(name, &[])` empties a module. Untyped functions,
  `HostSignature::any(name)`, take `(*args: Any, **kwargs: Any)` and report
  the arguments as given.
- **Between cells only.** `define` refuses while a cell is stopped
  (`DefineError::Busy`). It validates the whole module first
  (`DefineError::Invalid` with the reason), so a refused definition changes
  nothing; `DefineError::Raised` carries a `MemoryError` when it does not
  fit the memory limit. A module named like a bundled module (`json`,
  `json.decoder`, `builtins`, enabled in this build or not) is refused;
  names under a bundled package that are not its modules (`json.tools`)
  are the host's.
- **Size.** A module may have any number of functions, classes and
  exceptions. The Python that typed functions and classes are made of is
  compiled once per `Engine` and reused by later sessions of that Engine
  and its clones: `Session::new` plus `define` of 100 typed functions takes
  about 0.9 ms warm, against 6 ms cold \[M\].
- **Persistence.** Definitions survive `reset` and are part of snapshots and
  templates; `session.host_modules()` returns them. An image refers to the
  compiled host code by a hash of its source, and a restore takes it from
  the Engine's cache or regenerates it from the definition, refusing the
  image (`SnapshotError::Corrupt`) if the result differs: a restored session
  never runs other host code than it was saved with. Hosts that restore
  every image on a new Engine (one session per process or wasm instance)
  should use `engine.with_host_code_in_snapshots(true)`, which stores the
  code in the image (restoring 100 typed functions: 0.87 ms stored, 6.6 ms
  recompiled \[M\]). The mm-wasm guest and `mm-cell-worker` (a worker
  process for the cell protocol of the plak agent harness, spec §10.2) do.

A call from any frame (a loop, a comprehension, a sort key, a dunder, a
generator) converts its arguments to `HostValue`s and stops the cell with
`Progress::HostCall { call, function, name, args, kwargs }`. `name` is the
call name: `function` for builtins, `module.function`, or
`module.Class.method`. `function` is a `u32` id to dispatch on: `define`
returns a `HostFunctionRef { name, id }` per function (functions first,
then each class's methods), ids stay stable per call name for the session's
lifetime and across snapshots, and `host_function_id(name)` and
`host_function_name(id)` map between them. `call` increases per session.
Answer with one of:

- `Resume::Value(v)`: the call returns `v` in place. Calls are synchronous by
  default: models write `text = fs.read(p)`, and a forgotten `await` cannot
  happen.
- `Resume::Error(HostError::new(kind, message))`: the call raises
  `kind(message)` in place. `HostErrorKind` covers 18 built-in classes, from
  `ValueError` to `FileNotFoundError`; `from_name` maps a class name. See
  [Host errors](#host-errors) for payloads and host-named classes.
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
parse errors: of 396 model-written code-mode solutions against string
results (`conformance/codemode`), 311 parsed them with `json.loads` first
\[M\]. Raw text (a file, a log) stays a `Str`.

**Timeouts.** Time a host call out on the host side and answer
`Resume::Error(HostError::new(HostErrorKind::TimeoutError, "fetch timed out
after 30 s"))`, so the cell sees `TimeoutError`, the class models already
catch. For a whole cell, set a deadline: it parks rather than raising, so
the host decides whether to resume with a new deadline or `cancel()` and
report that the cell timed out.

### Typed host functions

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default()); let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?; let budget = 1_000_000_u64; let _ = (engine, code, budget); let _ = {
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
signature would, raising CPython's `TypeError`s at the call. Annotations
made of `None`, `str`, `bytes`, `bool`, `int`, `float`, `list`, `tuple`,
`dict` and host classes joined by `|` are checked (`list[str]` checks only
`list`); others only appear in the stubs. A mismatch raises `TypeError:
read() argument 'path' must be str, not int` at the caller's line.

The host then receives every parameter positionally, in declaration order,
with defaults filled and no `kwargs`: `fs.read('a')` arrives as
`HostCall { name: "fs.read", args: [Str("a"), None], kwargs: [], .. }`.
`*args` and `**kwargs` exist only in the untyped form. There is no static
check before the cell runs: for "type errors; nothing ran", run a checker
over `stubs()` on the host side first.

**One dict as the keyword arguments.** With `dict_as_kwargs: true`, a call
with exactly one positional `dict` and nothing else binds that dict's items
as keyword arguments, then checks them as usual: `fs.read({"path": "a",
"limit": 3})` is `fs.read(path="a", limit=3)`. This accepts both
conventions models write \[S\]. It is off by default because it is
ambiguous for a function whose only parameter is itself a `dict`. On a
method it applies after `self`.

Host functions have `__name__`, `__qualname__`, `__module__` and `__doc__`,
and `help(f)` prints the stub `stubs()` renders for it.

`mm-cell-worker`'s cell protocol carries only untyped tool names; a host
that wants typed functions or host classes embeds the crate.

### Host classes

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default()); let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?; let budget = 1_000_000_u64; let _ = (engine, code, budget); let _ = {
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
and refuse assignment; they are `tuple` subclasses, like named tuples.

An instance crosses the boundary in either direction as
`HostValue::Object { class: "files.Entry", fields: [("path", ..), ("size", ..)] }`,
with fields in the class's order. `class` is the module-qualified name, or the
bare name for a class of the builtins module. Methods are typed host
functions: `e.read()` reports
`HostCall { name: "files.Entry.read", args: [Object {..}, ...], .. }`. An
`Object` of an undefined class, or with other fields, raises `TypeError`
where the host answer arrives. There are no proxy objects whose attribute
access calls the host, so every value stays snapshot-safe: a handle (a job,
a file revision) is a host class whose fields hold an id.

### Host errors

`HostError` has two optional fields beside `kind` and `message`: `data`, a
`HostValue` that becomes the exception's `data` attribute, and `class`, a
host exception class that replaces `kind`. A host exception class is
declared with the module, as a subclass of one of the `HostErrorKind`
classes:

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default()); let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?; let budget = 1_000_000_u64; let _ = (engine, code, budget); let _ = {
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
  like any value.
- An unknown `class` raises `TypeError: unknown host exception class 'x'` at
  the call: the mistake is the host's.
- Failures of the bridge itself stay plain built-in exceptions: an unknown
  name is `NameError`, a refused argument `TypeError` before the call, a
  missing OS hook `PermissionError` (filesystem) or `RuntimeError` (any
  other).

Prefer raising for failures and returning values for expected outcomes such
as "not found" in a search: models check a returned error less reliably
than they catch an exception \[E\].

### Values

| Python | `HostValue` | Notes |
|---|---|---|
| `None`, `bool`, `float`, `str`, `bytes` | `None`, `Bool`, `Float`, `Str`, `Bytes` | |
| `int` | `Int(i64)`, or `BigInt(String)` in decimal past `i64` | |
| `list`, `tuple` | `List`, `Tuple` | a value that goes host → guest → host comes back unchanged |
| `dict` with `str` keys | `Dict(Vec<(String, HostValue)>)` | insertion order kept |
| named tuple | `Record { name, fields }` | a `Record` from the host becomes a named tuple instance; its name and field names must be ones `collections.namedtuple` accepts without renaming (identifiers, not keywords; fields not starting with `_`, none repeated), else `TypeError` in the guest with namedtuple's message |
| host class instance | `Object { class, fields }` | see [Host classes](#host-classes) |
| anything else (sets, functions, instances, non-`str` keys) | none: strict mode raises `TypeError` in Python before the call | with `HostArgs { lenient: true }` it arrives as `Unconverted::{Opaque, KeyNotStr, TooDeep, TooLarge}` |

### Stubs

`session.stubs()` returns `.pyi` text as `(module, text)` pairs, derived from
the same `HostModule` values: first `""` for the builtins, then one per host
module, by name, each listing its constants, classes and functions in
definition order. Typed functions and classes show their signatures and
docstrings; untyped functions take `*args: Any, **kwargs: Any` and return
`Any`; the OS hooks are left out. For the `files` module above, the text
declares `class Entry` with `path: str`, `size: int`, `__init__` and
`def read(self) -> bytes`, and `def listdir(*args: Any, **kwargs: Any) ->
Any`. Use them in the model's prompt, or with an external type checker.

### The stdlib's OS hooks

The bundled stdlib has no ambient I/O. Its OS operations call untyped
builtins (functions of module `""`) defined under fixed names: `open`, the
`Path.*` family (`Path.read_text`, `Path.write_bytes`, `Path.iterdir`, ...),
`os.getenv`, `os.environ`, `os.getcwd`, `os.system` and others. These are
the only dotted function names `define` accepts; `Session::define`
documents the full list and argument shapes. When none is defined the
operation raises `PermissionError` (filesystem) or `RuntimeError` naming it:
`open() is not available: this sandbox has no ambient filesystem. Use a
tool for files, environment variables and commands` (the second sentence is
the `HintGroup::OsHooks` hint). Defining `os.urandom`, `time.sleep` or
`asyncio.sleep` replaces the built-in entropy, virtual sleep and virtual
async sleep.

The remaining ambient state is configurable:

- `set_wall_clock(Some(Box::new(|| secs_since_epoch)))` is what `time.time()`
  and `datetime.now()` read. The default is the system clock;
  `wasm32-unknown-unknown` has none, and `None` makes the readers raise
  `RuntimeError`. `time.sleep` advances this session's clocks without waiting.
- `set_entropy(Some(Box::new(|buf| ...)))` feeds `os.urandom`, unseeded
  `random`, `SystemRandom` and `uuid4`. The default is OS randomness.
- `set_local_timezone(Some(tzif_bytes))` sets the zone for `time.localtime`
  and naive `datetime` conversions. The default is UTC. Snapshots carry it.
- `set_zone_source(Some(Box::new(|key| ...)))` serves `zoneinfo.ZoneInfo(key)`:
  return the zone's TZif bytes, for example
  `std::fs::read(Path::new("/usr/share/zoneinfo").join(key)).ok()`, or
  `None` for an unknown key. Keys arrive validated as CPython validates them.
  Without a source only `ZoneInfo('UTC')` loads. The source is not session
  state, but the zones a session loaded are: a restored session answers
  them from its cache with or without a source.

## Driving a session

For the common case, a `HostHandler` answers each stop, and `Session::drive`
resumes the session until the cell completes or raises, or the handler
returns `Step::Stop`:

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default()); let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?; let budget = 1_000_000_u64; let _ = (engine, code, budget); let _ = {
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

- **The session is lent to the handler at each stop**, so it can snapshot
  it, refuel it or look up ids before it answers. `Step::Stop` makes `drive`
  return that `Progress` unchanged. A wrong answer is a `ResumeError`, and
  the session stays at that stop.
- **`AsyncHostHandler` with `run_async`/`drive_async`** is the same for
  async hosts, on any executor. Handler methods return `BoxFuture<'_, Step>`
  (a pinned, boxed `Send` future), so a tokio host can await a tool call
  before it answers.
- **Fuel slices keep the executor responsive.** The async driver runs the
  cell in slices of `ASYNC_SLICE_FUEL` (10,000) fuel units: when a slice
  runs out, the future wakes itself and returns `Pending`, and the next poll
  continues where it parked, so sessions on one thread interleave. Slicing
  does not change the session's fuel budget, which is charged exactly; only
  its own exhaustion reaches `parked`.
- **A collection step ends a slice too.** `drive_async` turns on
  `Session::set_gc_yield`: a slice also ends after a safepoint that ran a
  step of a garbage collection, which charges no fuel. A host that slices
  with `set_fuel` itself can turn it on too; a `Fuel` park with fuel left is
  such a yield. It changes no results and is not in a snapshot.

Neither handler needs a feature or a dependency. `drive` takes
`&mut dyn HostHandler`.

## Parking, deadlines and cancel

A cell parks at a safepoint: a call, a backward jump, a step of a long
native operation, or a check the compiler puts in long straight-line code.
The engine looks at the fuel, the deadline and park requests every 256
safepoints, or at every one once less fuel than that is left, so a park
request or deadline is noticed within 256 calls or loop iterations.
`Session::fuel()` read from a host call is the fuel as of the last look; at
a park it is exact. Parking works at any depth and in any frame kind,
including generators, coroutines, dunders and comprehensions. Every frame
stays in the arena, so parking costs nothing and `Resume::Continue`
continues exactly where the cell stopped.

| Reason | Trigger |
|---|---|
| `ParkReason::Fuel` | fuel ran out: call `set_fuel(Some(n))`, then `Continue`. With `set_gc_yield(true)`, also after a collection step, with fuel left: `Continue` as is |
| `ParkReason::Deadline` | `set_deadline`'s instant passed |
| `ParkReason::Requested` | `ParkHandle::request()` from any thread (`session.park_handle()`, `Send + Clone`), or `request_park()` |

`Session::cancel()` ends a stopped cell (parked, at a host call or waiting)
without running more of it. No `finally` blocks run and pending futures are
dropped. Globals keep what the cell assigned, so the session stays usable. To
stop a cell running on another thread, request a park, then cancel the
parked cell.

A park request stays until a cell parks for it. Neither `start` nor
`cancel` drops it, so a host that requests a park and then sees the cell
end on its own gets one more `Parked { reason: Requested }` from the next
cell, and answers it with `Continue`. `Session::phase()` says what the
session is stopped at (`Idle`, `HostCall`, `Waiting` or `Parked`), and so
which `Resume` it takes. `Session::pending()` re-reports the stop itself, as
`start` or `resume` reported it (a host call with its id, function, name and
arguments, the calls still pending, or the park reason); `None` between
cells. Snapshots carry it, so a restored session says which call it waits
for and with what arguments.

## Messages for models

What a model reads after a failure decides its next attempt, so the engine
keeps CPython's wording and adds a sentence where the sandbox differs.

**Redirect messages.** An import of something micro-monty does not have
keeps CPython's `No module named 'x'` and adds, in the report only (not in
`str(e)`, like CPython's "Did you mean"), one sentence for its group. The
`OsHooks` sentence ends the refused hook's own message, so `str(e)` carries
it too.

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
| `Asyncio` | a name `asyncio` lacks; its streams get `Network`, and `create_subprocess_*`, `to_thread`, `run_coroutine_threadsafe` get `Processes` | micro-monty's asyncio has ALL_COMPLETED, Barrier, ... ; pass coroutines to asyncio.gather (the list is generated) |
| `OsHooks` | a refused OS hook (`open`, `Path.*`, `os.environ`, `tempfile`) | Use a tool for files, environment variables and commands |

`Session::set_hint(group, Some(text))` replaces a sentence (`{module}`
stands for the module), `Some("")` drops it and `None` restores the
default; a code-mode host might say "use the nested tools". The choice is
session state: it survives `reset` and snapshots.

**Did you mean.** A missing module attribute gets CPython 3.14's
suggestion (`AttributeError: module 'json' has no attribute 'lods'. Did
you mean: 'loads'?`), for bundled and host modules alike, and so does
`from json import lods`.

**Coroutines never awaited.** When a cell completes or raises, the engine
writes `<sys>:0: RuntimeWarning: coroutine 'fetch' was never awaited` to
stderr for each coroutine or host future of the cell that was never
started, scheduled or awaited. It is a scan at the end of the cell, not a
finalizer, so it does not depend on GC timing.

**What already ran.** `Session::call_log()` lists the host calls of the
running cell, or of the last one, as `HostCallRecord { call, function,
name, status }` in call order. `CallStatus` is `Returned`, `Raised`,
`Pending` or `Abandoned` (the cell ended before an answer). `start` begins
a new log, and snapshots carry it, so a host can tell the model which calls
ran before a failure and are not undone.

**Environment summary.** `Engine::environment()` returns a few lines for a
prompt, generated from the build: the modules that import, what is never
available, the asyncio surface, the limits and the calling conventions.
Add `Session::stubs()` for the host's own functions.

**Long results.** A completion's `repr` and `str` are cut at the text limit
(1 MiB by default, or the memory limit if that is less): `TextCut::Head`
keeps the beginning, `TextCut::HeadTail` both ends around `... [N bytes
omitted] ...`, and a list, tuple, dict or record keeps its shape
(`[{'id': 0}, {'id': 1}, ...(+998 items)]`). `Completion::value` keeps the
whole `HostValue`, so the host can store it and hand the model a reference.

**Long exception reports.** `PyException::message` and `traceback` are
bounded the same way (and rendering also stops when the deadline passes);
`truncated` is `true` when either was cut. A report under the limit is
exactly CPython's.

## Session templates

A template is a session saved after its setup (host definitions, a
bootstrap cell, common imports) and restored for each new session:

```rust
# use micro_monty::*; fn main() -> Result<(), Box<dyn std::error::Error>> {
# let engine = &Engine::new(EngineConfig::default()); let session = &mut Session::new(engine)?;
# let code = &engine.compile("x = 1\n", "cell.py")?; let budget = 1_000_000_u64; let _ = (engine, code, budget); let _ = {
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
- **Capture with the clock the instances use:** `time` keeps import-time
  readings, so an earlier clock makes `monotonic()` stand still.

**Not in the image**, as after any restore: the host-side settings (pass a
`SessionSettings` to `instantiate_with(settings)`; `instantiate()` uses the
defaults), the deadline and `park_handle()` (take it again), and a cell
budget other than the engine's `Limits::fuel`. **Kept in the image:**
module dunders, `set_last_expr_value`, `set_host_args`, `set_text_limit`,
`set_hint`, `set_local_timezone`, host definitions, what is left of the
fuel total and the limits (the memory limit becomes the smaller of the
image's and the restoring engine's).

**Invalidation.** A template image carries the build id like any snapshot,
and a build with another id refuses it (`SnapshotError::Incompatible`).
Rebuild templates from their setup code whenever the build id changes
instead of draining them. Key them by build id and setup code, not by their
bytes: captures differ where imports read the clock or entropy. A template
written natively restores on both wasm targets of the same build id.

**When it pays.** With a warm module cache, instantiating a template that
imported json, asyncio, re, collections and datetime takes about 1.1 ms,
and one with decimal, statistics, zoneinfo, html.parser and tomllib about
2.7 ms \[M\]. Use one when most cells import its modules, especially heavy
ones: a broad template made the code-mode suite's median cell slower (2.1
→ 4.6 ms), because most cells import only `json` \[M\]. There is no pool:
instantiate on demand, or keep spares on your own executor.

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

A snapshot is the session's full image: a garbage collection runs, then the
heap's live objects, code, frames, tasks, pending host calls with their
ids, globals, limits and settings are written. Restore validates every
structure before use (bounds, block headers, bytecode, frame states) and
charges the result against the restoring engine's memory limit
(`TooLarge`). A fresh session's snapshot is about 41 KB and a parked loop's
about 42 KB; with `re`, `random` and `time` loaded, about 282 KB \[M\].

The host still owns some state, because no engine can make it durable:

- Output: what was printed before the snapshot stays printed.
- In-flight host calls: a restored session waits for the same `call` ids,
  and `pending()` reports the call it is stopped at, with its arguments.
  Whether the side effect already happened is the host's to know.
- Host-side settings: the output, clock, entropy and zone source
  callbacks, the text cut and `int_bits_text` are a [`SessionSettings`]
  value, which `Session::new_with`, `Engine::restore_with` and
  `SessionTemplate::instantiate_with` take. Build it in one function and
  pass it to each (`restore` uses the defaults: no output, no zone source).
  Set the deadline and take the park handle again.
- Work done after the snapshot is redone after a restore.

### Storing snapshots

Images are not compressed, and the engine writes only full images. Make
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

   It is byte-identical across processes and targets for the same build
   \[M\] (on mm-wasm: `mm_session_new` with default options, then
   `mm_snapshot`). Store it, or regenerate it at start-up.
2. **Compress each image with zstd level 1, the base as a raw prefix and the
   frame checksum on**: `ZSTD_CCtx_refPrefix` / `ZSTD_DCtx_refPrefix` in
   libzstd, `zstd -1 --check --patch-from=base.mmsnap` on the command line.
   Decompress with the same base. zstd-1 alone shrinks parked sessions about
   3.6× and large heaps of small dicts 17–19× \[M\]; the base prefix makes
   small parked sessions another 1.3–1.4× smaller (code-mode parks' median
   102.8 KB → 19.7 KB) \[M\].
3. **Store with each blob:** the build id (`micro_monty::build_id()`,
   `mm_build_id`), which names the base, and how it was compressed. A
   raw-prefix frame does not record its prefix.
4. **Optional:** a zstd dictionary trained on your own parks, used together
   with the base, reached 8.1× against the full image on held-out code-mode
   parks, against 4.0× for zstd-1 alone \[M\]. Version it per build like
   the base.

An image's build id is its bytes 10..42 (after the 8-byte magic and the
2-byte format version). With the frame checksum on, zstd refuses a frame
decoded with the wrong base \[M\]; without it, `Engine::restore` refuses
the wrong bytes on the image's own checksum (`SnapshotError::Corrupt`).

### Compatibility: the build id

A snapshot carries a 32-byte **build id**: a hash of what the image's
meaning rests on (the snapshot format and codecs, the engine's declared
layouts, its tables, the prelude's and enabled stdlib modules' bytecode,
the stdlib features, and what `Session::define` generates; spec §9.3).
It does not cover the version or other function bodies: two builds that
differ only there share an id and restore each other's snapshots.

A snapshot restores only on a build with the same build id.
`Engine::restore` returns `SnapshotError::Incompatible { format, build_id }`,
naming the image's format version and build id, otherwise.
`micro_monty::image_build_id(&image)` reads the id without restoring, and
`SNAPSHOT_FORMAT` is this build's format. Snapshots are never migrated, and
source is never re-run to rebuild state.

- Pin the micro-monty revision and features of anything that must read
  your snapshots, and store `micro_monty::build_id()` beside each one.
- Stdlib features are part of the id. `mm` enables the four opt-in modules
  (`uuid`, `unicodedata`, `html.entities`, `ast`), so its id differs from a
  default build's. Cargo unifies features across a build, so a binary built
  together with `mm` gets mm's modules and mm's id.
- `mm version`, `mm-cell-worker --version` and the wasip1 `roundtrip`
  example print the id. Release notes list it for every shipped binary.
- Corruption and truncation are `SnapshotError::Corrupt`, never a panic. The
  stop the header names (idle, a host call, a wait, a park) must match the
  VM (`Corrupt { what: "stop" }`), and a waiting image must answer every
  pending call (a cancelled future counts). An engine panic while restoring
  is `SnapshotError::Internal`: the image may be sound, so keep it.

### Upgrading: drain old builds

The engine never migrates a snapshot. An upgrade that changes nothing the
build id covers keeps the id, and its old images restore on the new build.
When the id changes, keep the old build available until its parked
snapshots have drained, and treat a cell that no available build can
resume like a crash (an unknown outcome the host reconciles).

1. Store each image with its build id (bytes 10..42 of the image,
   `micro_monty::build_id()`, `mm_build_id`).
2. On upgrade, compare the new build's id with the old one's. The same id
   needs nothing more. Otherwise start new sessions on the new build and
   keep the old build loaded or installed, keyed by build id.
3. Restore each stored image on the build its id names. When every image of
   the old build has completed, been cancelled or been given up, retire it.
4. An image whose build is no longer available (or that fails with
   `SnapshotError::Incompatible`) is a crashed cell: report it as such, do
   not retry it on another build and do not re-run its source.

Ways to keep the old build:

- **wasm:** one `mm_wasm_host::Runtime` (or `hosts/js` `Runtime`) holds
  several guest modules keyed by build id. `Runtime::load` adds one and
  makes it current for new sessions, `Runtime::restore` picks the module
  the image's id names (`Error::UnknownBuild(id)` when none is loaded), and
  `Runtime::unload` retires a drained build. An old build is an old `.wasm`
  file.
- **Native, in-process:** a process links one micro-monty, so keep the old
  host binary (or the old `mm-cell-worker`) running or installed beside the
  new one, and route old images to it until they drain.
- **Native snapshots through the old build's wasm guest.** Native and wasm
  builds of the same source and features share a build id, so a native host
  can drain old snapshots under `mm-wasm-host`, if it keeps each shipped
  build's guest, built from the same revision and features.

### Cross-target restore

Nothing in the build id depends on the target, so a snapshot written on
one target restores on another with the same id:

- The ids agree across x86_64 Linux, aarch64 Linux and wasm32 \[M\], and
  images restore across native, wasip1 and unknown-unknown in every
  direction (`crates/mm-wasm-host` tests; `cargo xtask durability
  --cross-target` continues every own case's parks on the other target).
  The format is explicit little-endian; guest counts and lengths are 64-bit
  on every target (`sys.maxsize` is `2**63 - 1` on wasm32 too).
- Memory accounting charges native struct sizes, which differ between 32-
  and 64-bit targets, so a session near its limit on wasm32 can be
  `TooLarge` on a 64-bit target; leave headroom. Text and host-argument
  limits clamp to the 32-bit range on wasm32.
- On `wasm32-unknown-unknown`, set the wall clock and entropy again after
  restoring.

## Embedding guarantees

What a host running sessions in its own process can rely on, each tested.

**Panics: build with `panic = "unwind"`.** No guest input should panic:
clippy denies `unwrap`, `expect`, `panic!` and `todo!` in engine crates,
and the libFuzzer targets in `fuzz/` look for the rest. If an engine bug
panics anyway, then with unwinding the facade catches it, that session is
poisoned, and every other session keeps working (`tests/panics.rs`). This
workspace's `panic = "abort"` and the shipped `panic=immediate-abort`
(spec §6) kill the whole host instead, so an in-process embedder sets:

```toml
[profile.release]
panic = "unwind"
```

Builds for a wasm runtime or a child process, where that boundary contains
the fault, can keep the abort builds.

**A bounded Rust stack.** Running code keeps its nesting in the arena, and
walks over nested data use explicit stacks, raising `RecursionError` at the
depth limit. Parsing and compiling recurse natively, bounded by the nesting
limit ([Code size limits](#code-size-limits)): compiling at that limit
needs 650 KiB of thread stack in release and 3.7 MiB in debug, running a
session 40 KiB \[M\]. Give threads that run sessions at least 256 KiB and
threads that compile at least 1 MiB in release builds.

**Guest-driven allocations are charged to `Limits::memory`.** The arena,
code, frames, scheduler tables, collector scratch, host-module definitions
and the `HostValue` trees of host-call arguments and completions reserve
from the session budget before they allocate, and fail with `MemoryError`
(spec §11). `Session::compile` reserves 512 bytes per source byte + 16 KiB.
Not charged, but bounded otherwise: `Engine::compile` (at most 512 bytes per
source byte + 8 MiB \[M\]; compile model-written cells with
`Session::compile`), snapshot buffers (`snapshot_with_limit`) and output
(your callback).

**The Rust heap stays within 2.5 × `Limits::memory` + 16 MiB while a cell
runs, for any guest input.** That is the arena, about one transient native
copy of the largest value a native reads, and fixed tables. The measured
worst case is 2.01 × the limit over 73 probes at 16, 64 and 256 MiB \[M\];
`cargo xtask check` replays them with a counting allocator, and the
`memory_bound` fuzz target searches for inputs that break the bound.
Writing or restoring a snapshot holds the image once on top: at most the
limit + the image + 16 MiB \[M\]. Give a wasm instance about 3 × the session
limit + 16 MiB of linear memory, which covers both; a guest that runs out
of linear memory aborts instead of raising `MemoryError`.

**The module cache is the Engine's.** The first session of an `Engine` that
imports or restores a bundled module decodes its bytecode, which the Engine
keeps for every later session of it and its clones, on any thread. A
session is charged the same with the cache cold, warm or off, so its
results do not change (`tests/sessions.rs`). The cache lives outside every
session's limit, up to `Engine::with_module_cache(config, max_bytes)`
(`Engine::new`: `DEFAULT_MODULE_CACHE`, 8 MiB; 0 turns it off; past it a
session keeps its own copy). `Engine::module_cache_bytes()` reports its
size, host code included; sizes and savings are in spec §10.3.

**Bound CPU with fuel slices.** Run cells with a fuel budget and resume them
slice by slice. On `Parked { reason: Fuel }`, read `Session::fuel_wanted()`
before granting more: it is non-zero when the next native operation (a
large int operation, a big-int builtin, `sorted` of a long list) parked
before starting because it needs more fuel than was left, and resuming runs
it whatever fuel is then set. A host that enforces a budget stops the cell
instead when the wanted fuel is more than the cell may still spend. Any
other native operation runs in bounded steps: a slice is overdrawn by at
most 2^21 + 2^17 units, and a park request or deadline waits at most about
50 ms at the default limits on every measured row \[M\]
(`crates/micro-monty/tests/steps.rs`).
Outside that bound are full garbage collections over a large heap, about
0.4 ms per MiB live natively and 1.3–1.6 times that on wasm \[M\]. Count
what a cell was charged from a fuel total (`set_fuel_total`; refuel with
`Some(n)`, not `None`), not granted fuel less `fuel()`, which stops at 0
and misses overdrafts; `hosts/js`'s driver does.

**A snapshot or restore is not a step.** `Session::snapshot` collects in
full first and cannot park: about 1.4 ms per MiB of image natively, under
wasmtime and on V8 (about 250 ms at 170 MiB) \[M\]. A restore costs about as
much: 1.2–1.5 ms per MiB of small objects natively, 0.6–0.9 of large
blocks, up to 1.7 for a process's first restore \[M\] (wasm: spec §9.3).
A park request or deadline stops neither; a host timeout must not cover
them either: cutting one loses the image, and in wasm the instance.

**By target.** What bounds a cell's CPU and memory (spec §5.1, §11):

| | Native (the facade) | wasmtime (`mm-wasm-host`) | Node (`hosts/js`) | Cloudflare Workers (`hosts/js`) |
|---|---|---|---|---|
| Longest step between two safepoints | 50 ms at the default limits on every `Bounded` row of `tests/steps.rs` \[M\] | about 1.5–3× native \[E\]; a park request not honoured within 100 ms + 0.75 ms per MiB of the memory limit (292 ms at 256 MiB) traps the instance | no time bound: one step at most 102 ms at a 16 MiB limit, 1.7 s at 256 MiB \[M\] | as Node: 132 ms / 2.1 s \[M\] |
| Fuel overdraft per slice | at most 2^21 + 2^17 units | the same: fuel counts are identical across targets | the same | the same |
| Deadline | noticed at the next check, never inside one step | the same, plus the grace trap above | on `performance.now()`; only the platform stops one long step | never fires while a cell runs: the clock stands still during synchronous wasm, so fuel is the only CPU bound |
| Memory, `Limits::memory` = L | Rust heap at most 2.5 × L + 16 MiB | linear memory about 3 × L + 16 MiB; it never shrinks | as wasmtime | as wasmtime |
| Time per fuel unit, bench programs | 0.014–0.25 µs \[M\] | 0.032–0.47 µs \[M\] | 0.024–0.38 µs \[M\] | 0.029–0.45 µs in workerd \[M\] |

So CPU per cell on V8 is at most about a fixed part (compiling, imports: up
to 8 ms for the 22 code-mode tasks) + 0.5 µs × the fuel budget (the dearest
native, `str.upper`, takes 0.48 µs a unit in workerd \[M\]) + one native
step + a collection (5 ms at 16 MiB, 85 ms at 256 MiB) \[E\]. Results do not
depend on how a host slices the run: output, exceptions and values are the
same unparked, in fuel slices of any size and across restores on either
target (`cargo xtask durability`), except for a `MemoryError` a snapshot
moves in an operation not retried and for address-derived values such as
`id()` ([Other differences from CPython](#other-differences-from-cpython)).

### Many sessions in one process

The facade registers no global allocator and starts no threads. On glibc's
`malloc`, one thread per session is costly: each thread that allocates gets
an arena of its own (up to 8 per core), and freed memory stays in it. 64
sessions in one process, each on its own thread, replaying recorded agent
cells with about 14 MB of live engine heap among them \[M\]:

| Host allocator | RSS under load | RSS with no sessions left | Per-cell p99 |
|---|---:|---:|---:|
| glibc | 702 MiB | 609 MiB | 4.1 ms |
| glibc, `MALLOC_ARENA_MAX=2` | 333 MiB | 242 MiB | 280–400 ms |
| mimalloc | 418 MiB | 48 MiB | 3.8 ms |
| jemalloc | 385 MiB | 35 MiB | 3.8 ms |

Register mimalloc or jemalloc as the host binary's `#[global_allocator]`,
or run sessions on a fixed pool of threads (a session stopped on one thread
resumes on another), so the number of arenas follows the pool.

`mm-wasm-host` gives every session and restore a new instance, and with many
sessions in one process its page faults make a snapshot and restore cost
far more than natively: 4.9 ms per swap at 8 sessions and about 144 ms at
64, against 1.6 ms natively (0.4 MiB images) \[M\]. Spread wasm sessions over
processes, or set `RuntimeConfig::pool_instances` (wasmtime's pooling
allocator; at 64 sessions a 128-slot pool took 15.3 ms per swap at a peak
RSS of 1,153 MB, against 799 MB on demand \[M\]). Size the pool at about
twice the sessions that swap; with every slot in use, a start or restore
fails with `Error::Trap` before any guest code runs and can be retried.

## The `mm` command line

`mm` runs Python files on micro-monty. It is built with the four opt-in
stdlib modules (`uuid`, `unicodedata`, `html.entities`, `ast`) and the
default limits (256 MiB, no fuel limit). Get it from a release's archive
(`micro-monty-<target>.tar.gz` for macOS arm64 and x86_64 and Linux x86_64
and arm64, with `mm-cell-worker` beside it), or build it:

```text
cargo install --locked --git https://github.com/N0xMare/micro-monty mm-cli
```

```text
mm run FILE
mm version
mm exec --json [--recursion-limit N] [--host-fixtures] [--report-fuel]
               [--main-module] [--snapshot-every N --snapshot-dir DIR] FILE
mm resume --json DIR
```

- **`mm run FILE`** runs the file as `python FILE` does: `__name__` is
  `'__main__'` and `__file__` its absolute path. Output goes to stdout and
  stderr in order, compile warnings and an uncaught exception's traceback
  (CPython's format) to stderr. It exits 0, 1 on an error, and 2 when the
  file cannot be read or the arguments are wrong. The value of a last
  expression is not printed.
- **`mm version`** (or `--version`) prints `mm <version>` and `build_id
  <hex>`: `mm resume` restores only snapshots written under the same id.
- **`mm exec --json FILE`** prints one JSON object describing the run:
  `stdout` (stdout and stderr merged), `result` (`repr`, `str` and `type`
  of a final expression statement, or `null`), `error` (`type`, `message`,
  `traceback`, or `null`) and `exit` (1 when there was an error). The
  process exits 0 once it has printed a report. The file runs in a
  namespace without `__name__` unless `--main-module` sets both dunders.
  `--recursion-limit N`, right after `--json`, sets the frame depth limit;
  `--report-fuel` adds `fuel`, the units an unlimited run used.
  `--host-fixtures` provides the host functions and small virtual
  filesystem of Monty's test fixtures, for the conformance corpus.
- **Durable runs.** With `--snapshot-every N --snapshot-dir DIR`, `exec`
  runs N fuel units per slice. At each park (and with `--host-fixtures`, at
  every host call and every wait on futures) it saves the session to
  `DIR/session.mmsnap` and the host's side (output so far, the answer owed)
  to `DIR/state.json`, prints `{"park": K, "reason": ..., "bytes": ...,
  "stdout": ...}` and exits. **`mm resume --json DIR`** restores both in a
  new process and continues to the next park, or prints the report `exec
  --json` prints for an unparked run. `cargo xtask durability` drives this
  over every passing case.

`TZ` sets the local time zone (a zoneinfo name, a path, or a POSIX rule;
UTC without it), and `zoneinfo` reads `$TZDIR` or `/usr/share/zoneinfo`. `mm` defines no other host functions, so `open()`,
`os.getenv` and the like raise. The `MM_GC_STRESS`, `MM_GC_YIELD` and
`MM_STEP_MS` variables are for the engine's own test gates.

## WebAssembly hosts

`crates/mm-wasm-guest` builds micro-monty as a wasm module for
`wasm32-unknown-unknown` or `wasm32-wasip1` speaking the `mm-wasm`
interface (`crates/mm-wasm-abi`, `ABI_VERSION` 1): a host sizes an input
buffer, calls exports such as `mm_session_new`, `mm_define`, `mm_start`,
`mm_resume` and `mm_snapshot`, and answers the `mm_host` imports (clock,
park poll, wall clock, entropy, output, time zones). The guest reads no
WASI clock, randomness or files. micro-monty is 0.0.x with no compatibility
layers: any change to the interface, a new option tag or optional export
included, bumps `ABI_VERSION`, and a host and a guest must agree on it. A session's options (its limits, and the settings
an image does not carry) are a tagged record that `mm_session_new` and
`mm_session_restore` both take. Snapshots move between native and wasm
sessions of the same build id.

Two guests are shipped:

- **`mm_wasm_guest.wasm`**, the default guest: the stdlib and prelude
  bytecode is in its data segments.
- **`mm_wasm_guest_bundle.wasm`**, the bundle guest: the stdlib and prelude
  bytecode is in the custom sections `mm.prelude` and `mm.stdlib`, which the
  host reads once per loaded module and serves through the import
  `mm_host.bundle_read`. It loads a module's bytecode when a session first
  needs it; a typical cell's 5 modules (55 KB) take 26 µs to read from
  memory \[M\]. Its data per instance falls from about 450 KB to about
  120 KB, and on V8/workerd each idle session uses about 340 KiB less memory
  \[S, measured 2026-11-09\]. A host must provide `mm_host.bundle_read` to
  run it: without that import it fails to instantiate. Both guests of a
  build have the same build id, so snapshots cross between them.

**On V8 and Workers, use the bundle guest.** V8 copies a module's data
segments into every instance, and the bundle guest keeps the stdlib out of
them. Both reference hosts below provide `mm_host.bundle_read`.

The release workflow attaches both guests to each GitHub release
(`mm_wasm_guest-wasm32-unknown-unknown.wasm` and
`mm_wasm_guest_bundle-wasm32-unknown-unknown.wasm`; the v0.0.1 release
predates this). `cargo xtask size` builds them as shipped (release,
`panic=immediate-abort`, `wasm-opt -Oz` from binaryen, which `.agents/setup`
installs) into `target/wasm32-unknown-unknown/shipped/mm_wasm_guest.wasm`
and `target/mm-wasm-bundle/wasm32-unknown-unknown/shipped/mm_wasm_guest_bundle.wasm`;
restore snapshots on the guests of the release that wrote them.

**wasmtime: `crates/mm-wasm-host`.** A synchronous reference host with an
API close to `Session`: one `Session` owns one wasmtime store and instance,
so a trap loses that session only. It is its own cargo workspace and not
published; depend on it by path or git. `cargo xtask wasm-host` tests it
against both shipped guests.

```rust ignore
use mm_wasm_host::{Runtime, RuntimeConfig, SessionOptions};

let rt = Runtime::new(Path::new("mm_wasm_guest.wasm"), RuntimeConfig::default())?;
let options = SessionOptions::new().with_limits(Limits::new().with_memory(64 << 20));
let mut s = rt.session(&options)?;
s.set_output(Box::new(|_stream, text| print!("{text}")));
s.define(&HostModule::untyped("", &["lookup"]))?;
let progress = s.start("lookup('apple') * 3", "cell.py")?;   // a micro_monty::Progress
// ... answer with s.resume(&Resume::Value(..)), as natively
let image = s.snapshot(None)?;
let again = rt.restore(&image, &options)?;   // picks the loaded build by the image's id
```

`RuntimeConfig` sets the store's linear-memory ceiling (default 784 MiB,
for the default 256 MiB session limit), the park grace interrupt, an
optional per-call budget, a precompiled `.cwasm` cache beside the module
(on by default) and the pooling allocator. A park request
(`Session::park_handle()`) parks at the next safepoint check; the grace
interrupt traps only when one native operation or collection outlasts it.
After `Error::Trap`, restore the last image into a new session. `SessionOptions`' default host-argument
depth is 1000, where the native `HostArgs` default is 100.

**Node and Workers: `hosts/js`.** A TypeScript host for V8 that follows the
wasmtime host call for call, with an async driver for code mode
(async tools as futures, fan-outs answered in batches, fuel slices with
yields and a fuel budget). It is not published: copy `hosts/js/src` into
your project. Node 23.6 or later runs the sources as they are, and
Wrangler's esbuild strips the types for Workers (`hosts/js/README.md`).

```ts
import { untypedModule } from "./src/abi.ts";
import { Runtime, sessionOptions } from "./src/host.ts";
import { run } from "./src/driver.ts";

// Node: bytes. Workers: `import guest from "./mm_wasm_guest_bundle.wasm"`.
const rt = await Runtime.create(bytes);
const s = rt.session(sessionOptions({ limits: { memory: 32 << 20 } }));
s.define(untypedModule("", ["get_order"]));
const r = await run(s, "get_order(1)", {
	tools: { get_order: async (c) => ({ id: c.args[0] }) },
	fuelSlice: 10_000, // yield to the event loop every 10k fuel
	fuelBudget: 5_000_000, // stop the cell (before a big native operation) past this
});
const image = s.snapshot(); // store the image, drop the session
const again = rt.restore(image); // later, in any isolate with the same build loaded
```

V8 has no epoch interruption and no store limits: one native operation that
reaches no safepoint cannot be stopped from JavaScript, so bound CPU with
fuel slices and `fuelWanted()` (the driver does), and rely on the platform
as the last resort. On deployed Workers, `performance.now()` stands still
while wasm runs, so deadlines fire only after I/O; use fuel there.
`RuntimeConfig.sessionsPerInstance` lets one instance hold several sessions
(an idle session costs about 1.57 MB of linear memory alone in an instance,
about 0.1 MB as one more in a shared one \[M\]), but a trap then loses all
of them, and the instance's memory must hold the sum of their bounds.

## Binary size

Sizes of the shipped builds at v0.0.1 (`cargo xtask size`: release,
`panic=immediate-abort`, `wasm-opt -Oz` on wasm) \[M\]:

| Artifact | Bytes |
|---|---:|
| `hello.wasm` (the facade's example, `wasm32-unknown-unknown`) | 2,271,969 |
| native `hello` (x86_64 Linux) | 2,490,336 |
| `mm_wasm_guest.wasm` | 2,358,872 |
| `mm_wasm_guest_bundle.wasm` | 2,357,307 |

**Native embedders: link with `-Wl,-z,pack-relative-relocs`.** It takes
35,960 B off native `hello` by shrinking `.rela.dyn` from 37,368 B to under
1.3 KB \[M\], costs no instructions and needs glibc 2.36 or later (not in
the measured `hello`). In your `.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-Wl,-z,pack-relative-relocs"]
```

**An allocator for `wasm32-unknown-unknown` (optional).** There, std's
allocator is the `dlmalloc` crate, whose `free` and `realloc` check the
layout with assertions that fat LTO inlines into every deallocation site:
about 84 KB of `hello.wasm` and 97 KB of the guest \[M\].
`crates/mm-wasm-alloc` is the same dlmalloc without the checks. To use it,
register it in the binary (a git or path dependency; not published):

```rust ignore
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[global_allocator]
static ALLOCATOR: mm_wasm_alloc::Allocator = mm_wasm_alloc::Allocator::new();
```

It is empty on other targets and is for single-threaded modules (no
`atomics` target feature, rustc's default).

## Optional stdlib modules

The default build bundles `json`, `re`, `collections`, `itertools`,
`functools`, `datetime`, `random`, `asyncio`, `typing`, `base64`, `hashlib`,
`dataclasses`, `operator`, `copy`, `heapq`, `bisect`, `string`, `textwrap`,
`enum`, `abc`, `binascii`, `time`, `os`, `pathlib`, `fnmatch`, `posixpath`,
`csv`, `io`, `gc`, `types`, `contextlib`, `traceback`, `logging`,
`urllib.parse`, `pprint`, `shlex`, `difflib`, `struct`, `html` (with
`html.parser`), `warnings`, `codecs`, `platform`, `tempfile`, `decimal`,
`numbers`, `contextvars`, `fractions`, `statistics`, `tomllib`, `calendar`,
`hmac`, `zlib` (checksums only), `zoneinfo`, `tokenize` and `token`, plus
`sys`, `math`, `cmath` and the builtins, which are always built. The
engine's internal `_mm` is hidden: importing it raises `ModuleNotFoundError`.

Every bundled module has a `mod-*` feature on `micro-monty` (dots and
underscores become `-`: `mod-html-entities`; `urllib.parse` is
`mod-urllib`, and `token` and `html.parser` come with `mod-tokenize` and
`mod-html`). `uuid`, `unicodedata`, `html.entities` and `ast` are opt-in;
without one, importing it raises `ModuleNotFoundError: No module named
'ast'. 'ast' is not enabled in this build of micro-monty`. Without
`html.entities` (the default build), `html.unescape` resolves only numeric
references and `amp`, `lt`, `gt`, `quot` and `apos`. `ast` is CPython's `ast` over micro-monty's own
parser (`parse`, `dump`, `walk`, `NodeVisitor`, `NodeTransformer`,
`literal_eval`, ...; no `unparse` and no `compile()` of a tree) and adds
about 48 KB to `hello.wasm` \[M\]. To enable one, list it:

```toml
micro-monty = { version = "0.0.1", features = ["mod-uuid", "mod-ast"] }
```

To leave default modules out, turn off the default features and list the
modules to keep; a module brings the modules it imports (`mod-datetime`
brings `time`, `re`, `warnings` and `operator`). A few functions import a
module only when called and need it listed too: `time.strptime` needs
`mod-datetime`, `Fraction.from_decimal` `mod-decimal`,
`os.path.expandvars` `mod-re`, and `types.coroutine` on a plain function
`mod-functools`. Without `mod-asyncio` (about 51 KB of `hello.wasm` \[M\]),
`async def`, `await` and awaited host functions still work. Every module
feature changes the build id.

```toml
micro-monty = { version = "0.0.1", default-features = false, features = ["mod-re", "mod-collections"] }
```

### Smaller module sets for code-mode agents

Two lists measured against 2,841 cells from real code-mode sessions
(`conformance/codemode/real`) and 1,782 one-shot solutions to API and data
tasks (`conformance/codemode`); copy the one that fits. Leaving modules out
changes no speed \[M\].

**Code mode** (34 features): every module that at least 2% of either corpus
imports, every module a real cell that succeeds needs, and `textwrap`,
`hmac` and `zlib`. It saves about 85 KB of `hello.wasm` (4%), 89 KB of
native `hello` and 85 KB of the guest \[M\]. No real cell that succeeds on
the default build fails; 42 of the 1,782 synthetic solutions (2.4%) fail on
an import (`zoneinfo` 21, `calendar` 9, `tomllib` 9, `shlex` 3).

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

**Code review** (19 features; `mod-abc` comes with `mod-collections`): the
smallest set that every recorded real cell still succeeds on. It saves
about 211 KB of `hello.wasm` (10%), 221 KB of native `hello` and 211 KB of
the guest \[M\], but 549 of the 1,782 synthetic solutions (31%) fail on an
import, mostly `datetime`, `csv`, `statistics`, `html`, `urllib.parse` and
`decimal`. Use it only for agents that read and search code.

```toml
micro-monty = { version = "0.0.1", default-features = false, features = [
    "mod-json", "mod-re", "mod-collections", "mod-itertools", "mod-functools",
    "mod-random", "mod-asyncio", "mod-typing", "mod-base64", "mod-hashlib",
    "mod-operator", "mod-heapq", "mod-bisect", "mod-binascii", "mod-os",
    "mod-pathlib", "mod-fnmatch", "mod-io", "mod-struct",
] }
```

## Known differences from CPython

micro-monty aims at the Python that agents write, with output equal to
CPython 3.14 wherever a feature is supported: 546 of the 568 CPython-scored
cases of Monty's test corpus pass \[M\] (`conformance/results/micro-monty.md`).
Unsupported syntax fails at compile time with "micro-monty does not support
X".

### Left out by design

These follow from the sandbox and the snapshot contract (spec §1, §4).

- **Ambient I/O.** No filesystem, network, subprocess, clock or randomness
  except through host functions. `tempfile` imports, but everything in it
  that would create or find a file raises `PermissionError`, as `open()`
  does without a host `open`. `threading`, `socket`, `subprocess`,
  `multiprocessing`, `ctypes` and `sqlite3` do not exist.
- **Threads and `weakref`** would make scheduling or GC timing observable.
  `async` is the concurrency model. `gc.disable()` only sets the flag
  `gc.isenabled()` reports, and `gc.collect()` returns 0.
- **`__del__` and `__slots__`** are rejected at compile time, and so is a
  user `__getattribute__` in a class body (one added later is ignored).
  Objects that refuse new attributes in CPython (slotted classes,
  `datetime`) accept them here. `__getattr__`, properties and descriptors
  work.
- **`exec`, `eval` and `compile`** are not defined (`NameError ...
  micro-monty does not support exec()`); code arrives only as cells.
  `__import__(name, globals, locals, fromlist, level)` works over the same
  modules as `import`. **`globals()`** is a read-only view, not a `dict`:
  reads and iteration work as on CPython, and any write raises `TypeError`.
- **Imports** are limited to bundled and host-defined modules. `sys.path` is
  an empty list that imports never search; there are no packages on disk,
  no relative imports and no C extensions. `platform` reports fixed values
  (`system()` is `'Linux'`, `python_implementation()` `'micro-monty'`).
- **Surrogates.** A `str` is UTF-8, so it cannot hold a lone surrogate
  (U+D800 to U+DFFF). A surrogate escape in a literal (`'\ud800'`) is a
  `SyntaxError`, and `chr()`, `'%c'` and `format(n, 'c')` raise
  `ValueError` for one; write the character, or `'\U0001f600'`.
  `errors='surrogateescape'` raises `NotImplementedError` at the first
  undecodable byte, and `surrogatepass` decodes as `strict`. `json.loads`
  decodes an escaped pair to its character and raises `JSONDecodeError` at
  a lone surrogate escape.
- **`unicodedata.normalize`** and `is_normalized` cap a run with no
  normalization boundary at 65,536 characters after decomposition
  (`MemoryError` past it, where CPython's `is_normalized` returns `True`);
  UAX #15 text allows at most 30 non-starters in a row.
- **t-strings** (PEP 750) are rejected. Annotations are parsed and never
  evaluated. There is no type checker in the engine: host functions check
  their own arguments.

### Not supported

- PEP 695 `type` aliases (a compile error); `locals()`, and `vars()` without
  an argument; `memoryview`; `inspect`; a metaclass with `__prepare__`
  (other metaclasses work).
- **Evaluated annotations.** An annotation is kept as its source text:
  `C.__annotations__` is `{'x': 'int'}`, functions have no
  `__annotations__`, and `typing.get_type_hints` and
  `dataclasses.Field.type` give strings.
- **`bytearray`** works (constructors, item and slice assignment, the
  list-like mutators, the bytes methods, `struct.pack_into`), but not its
  subclasses, `bytearray % args`, bytearray host values, or bytearray
  arguments to `hashlib`, `re` and `json`.
- **Parts of bundled modules:** `logging.handlers`, `logging.config` and
  file handlers; `difflib.HtmlDiff`; `zlib` beyond `crc32` and `adler32` (no
  deflate; `gzip` and `zipfile` do not exist); `calendar` locales other than
  C; incremental and stream codecs (and codecs from `codecs.register` are
  not seen by `str.encode`); `zoneinfo.available_timezones()`; caller
  locations in log records. `hmac.compare_digest` is not constant-time.
- **Codecs** other than UTF-8 (and `utf-8-sig`), UTF-16, UTF-32, ASCII and
  Latin-1 raise `LookupError`.
- **Missing names:** the builtins `input`, `breakpoint`, `exit`, `quit`,
  `copyright`, `credits` and `license`; `sys.exit` and `sys.stdin`; the exceptions `BlockingIOError`,
  `BrokenPipeError`, `ChildProcessError`, `ConnectionError` and its
  subclasses, `InterruptedError`, `ProcessLookupError`,
  `PythonFinalizationError` and `UnicodeTranslateError`; the module
  `builtins`; `__builtins__`, `__spec__` and a module's `__doc__`;
  `sys.modules`, `sys.getsizeof`, `sys.intern` and `sys.getrefcount`;
  `function.__globals__` and `__code__`; `traceback`'s stack-walking
  functions; `string.Formatter`; `re.Scanner`; `functools.Placeholder`; the
  dunders of builtin types as type attributes (`dict.__or__`).
- **Subclassing** `frozenset`, `complex`, `property`, `classmethod`,
  `staticmethod`, `zip`, `enumerate` or `super`; `map`, `filter` and
  `reversed` are functions, not types.
- **Named Unicode escapes** (`'\N{BULLET}'`) are a `SyntaxError`.

**asyncio** follows CPython 3.14 for `run`, `gather`, `sleep`,
`create_task`, `wait_for`, `wait`, `as_completed`, `shield`, `TaskGroup`,
`timeout`/`timeout_at`, the locks, events, conditions, semaphores and
barriers, the queues, `Runner`, `current_task`, `all_tasks`, the running
loop's `time()`, `create_future`, `call_soon`, `call_later` and `call_at`,
`contextvars` contexts per task, cancellation and `ExceptionGroup`s. What
differs:

- Event-loop policies, threads (`to_thread`, `run_in_executor`),
  subprocesses, streams, sockets and eager task factories are not
  available; they raise with the `Processes` or `Network` hint.
  `asyncio.Future` is a function, not a type, and `run_forever()` raises.
- Time is virtual, and host calls take none of it: `wait_for` and
  `as_completed(timeout=)` fire only on `asyncio.sleep`s. Time out slow host
  calls on the host side.
- `create_task` outside a running loop does not raise; the task runs in the
  next `asyncio.run`. Tasks left at the end of `asyncio.run`, the timers
  behind `call_later` and `wait_for` included, are cancelled in creation
  order, "Task exception was never retrieved" is reported when it ends, and
  task reprs and reports have no `coro=<...>` or source locations.
- An unfinished async generator is never finalized; use
  `contextlib.aclosing`. Coroutines have no `cr_frame`, `cr_await` or
  `cr_code`.

### Other differences from CPython

- **Sorting.** `sorted` and `list.sort` are a stable merge sort that calls
  only `<`. For a total order the result is CPython's, but `__lt__` is
  called on other pairs and a different number of times, and a mixed-type
  list can fail on another pair first.
- **NaN.** Floats are NaN-boxed with one canonical NaN, so `[x] == [y]` is
  `True` for two `float('nan')` objects (CPython: `False`).
- **Deep comparisons** raise `RecursionError` past 1,000 levels whatever
  `Limits::recursion_depth` is.
- **Numbers.** `math.cbrt`, `exp`, `log`, the trigonometric and hyperbolic
  functions, `erf`, `gamma`, `atan2` and float `**` can differ from glibc's
  results in the last digit (108 of 92,000 random calls \[M\]). `range`
  bounds must fit in 64 bits: `range(2**63)` raises `OverflowError`.
- **Objects.** One object holds at most 64 MiB whatever the memory limit.
  `namedtuple` and dataclasses take at most 126 fields.
- **Iteration.** A `dict` subclass's `__missing__` is not called through
  `d.__getitem__(k)`.
- **`json.loads` with hooks** parses the whole document before calling any
  hook, and nesting deeper than CPython's recursion limit parses.
- **`dir()`** of a builtin type lists its method table plus a fixed set of
  dunders, not CPython's list exactly. `help()` prints a short stub.
  `UnicodeEncodeError` and `UnicodeDecodeError` have no `encoding`,
  `object`, `start`, `end` or `reason` attributes.
- **SyntaxError text** follows CPython 3.14, except that a misspelt keyword
  gets no "Did you mean", a missing comma gets no "Perhaps you forgot a
  comma?", and a few carets differ.
- **`MemoryError` beside uncollected garbage.** Most operations whose
  allocation fails wait for a full collection and run once more: copies,
  constructors over a container, slices and list comprehensions succeed
  where the result fits (three dropped `s.copy()` of a 2M-item set at 256
  MiB, as in CPython \[M\]). Not yet retried, being fixed in 0.0.2, so they
  can raise where CPython succeeds \[M\]: set and dict comprehensions and
  `add`/`append` loops (on their own outgrown tables), `bytes(n)`,
  `bytearray(n)`, a wide f-string field, `sorted` and `list.extend`. A
  call's `**kw` is never run again (its merge raises on a duplicate key). A
  retry's collection is charged, a unit per 64 bytes of `Limits::memory` up
  to 2^20, so a loop that catches `MemoryError` pays for each. A snapshot
  collects first, so it can flip an unretried outcome either way, and it
  moves fuel totals (six `f(*big, **kw)`: 11% less with a snapshot at every
  park \[M\]).
- **Fuel after a snapshot, long non-ASCII strs.** A snapshot's collection
  forgets the positions that make indexing a long non-ASCII str cheap, so
  the next walk is charged again, restored or not: `W[k]` for every 997th
  `k` below 1,600,000 costs 43,313 units unparked and 8,687,009 with a
  snapshot and restore at every 20,011-unit park \[M\]: use larger slices.
- **Address-derived values.** Which address a new object gets depends on
  when collections run (slice boundaries, snapshots, GC stress, the
  session's limits), so `id()`, default `repr`s and the order of sets of
  identity-hashed objects can differ between runs sliced differently \[M\].
  `id` stays unique among live objects and fixed for an object's life across
  parks and restores; `is` and value-hashed containers are unaffected.
- **Smaller ones.** `io.StringIO.seek` accepts a float; `bytes.hex(None)` is
  accepted; a set display of constants whose hashes collide can iterate in
  another order; a dataclass's `__doc__` is `None`; `re`'s ignore-case
  matching misses a few case folds (`ﬅ`/`ﬆ`, the Kelvin sign in a class
  range); `copy.copy` and `deepcopy` of an exception lose its `args` or
  raise `TypeError` (`ValueError('a')`), and of a set subclass its items;
  `OSError(2, 'x')` is not a `FileNotFoundError` \[M\].

### Code size limits

Source CPython compiles can fail to compile on micro-monty past these
limits, always with a `CompileError`, never a crash:

- **Nesting, 200 levels** (`RecursionError: maximum recursion depth
  exceeded during compilation`). Each bracket, call, conditional arm,
  unary operator, `**` and block counts one, a lambda two. A chain is one
  level whatever its length: operators (`a + b - c * d`), attribute,
  subscript and call chains (`x.f(1)[0].g()`), conditional expressions
  chained in their `else`, `elif` arms, f-string fields and string
  concatenation, up to 100,000 chain operators per expression.
- **255 registers per function** (`SyntaxError: micro-monty does not support
  expressions needing more than 255 registers`). Every value an expression
  holds while computing another takes one: unpacking into more than about
  250 targets, more than about 250 decorators on one definition, nested
  calls more than 85 deep or nested dicts more than 127 deep. Lists, tuples,
  dicts, sets, call arguments and f-strings of any length do not.
- **250 local variables per function** (`SyntaxError: micro-monty does not
  support functions with more than 250 local variables`). Module and class
  bodies are not limited.
