# Reset probes

`Session::reset` is a boundary (docs/spec.md §4, "A reset is a boundary"):
a cell after it reports what it reports in a new session with the same host
definitions. These files check it on the native facade
(`crates/micro-monty/tests/reset_boundary.rs`), through the reference
mm-wasm host (`crates/mm-wasm-host/tests/reset_boundary.rs`) and through the
JS host (`hosts/js/test/reset.test.ts`).

- `probes.txt`: one probe per way a cell was or could be able to reach the
  next one. Cell A, a reset, cell B; B's value or error, host calls and
  output must be those of B in a new session. A `>>>` line gives what A
  must end in: CPython 3.14's error for the same write to a builtin
  function, an immutable type or a struct sequence. `!==` probes are the
  documented differences and must differ.
- `walk.py`: the search for what the probes miss. It walks everything a
  cell can reach by attribute (`dir()` and the dunders it leaves out) and
  by item, six levels deep, from CPython's builtin names, the host's
  definitions, what the host's functions return and raise, and every module
  name CPython and mm-stdlib have. In `mark` mode it writes a mark to each
  object every way a cell can (an item, an attribute, `object.__setattr__`,
  `type.__setattr__`, a function's defaults, name and doc); in `check` mode
  it reports the marks it finds. It is one more probe: mark, reset, check
  must print what check prints in a new session. Without the reset the
  check finds the marks, which the native test asserts, so the walk is
  known to reach and write. It is deterministic: `dir()` is sorted and no
  clock or entropy is read.
- `sweep.py`: the search for native methods that write a `self` they did
  not check. It calls every callable in `dir()` of the builtin types
  (CPython's `dir(builtins)` that are types, and the types without a
  builtin name) with each shared object as `self` and ten argument sets,
  then a reset, and its check must print what it prints in a new session.
  With the `self` checks of `BaseException.with_traceback` and `add_note`
  taken out, the check differs [M]. It tries ten argument sets, not every
  one: a method that writes only for other arguments is not found.
- `probes.rs`: the host both Rust tests define (typed and untyped
  functions, a class, an exception class, list, dict and record
  constants and record defaults, in the builtins and in a module), its answers and the
  comparison. The JS test defines the same.

What the walk cannot see: state only native code reads and no attribute
shows (the recursion limit and the int-to-str digit limit have their own
probes), and objects past six levels or behind a call it does not make.

A new way to share state across a reset gets a probe here; a new host
definition kind or native module is covered by the walk once its name is in
`walk.py`'s lists (regenerate them from `dir(builtins)` and
`sys.stdlib_module_names` when the oracle moves).
