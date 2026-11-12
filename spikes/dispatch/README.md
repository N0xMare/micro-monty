# Spike S1: interpreter dispatch

Throwaway benchmark that decides micro-monty's bytecode shape (docs/spec.md
§8.1). Results and method: [`docs/spikes/dispatch.md`](../../docs/spikes/dispatch.md).
Nothing here ships; the package is its own cargo workspace and the root
`Cargo.toml` excludes `spikes/`.

Three bytecode designs × two dispatch mechanisms = six variants, on one shared
substrate:

| File | What |
|---|---|
| `src/value.rs` | NaN-boxed 8-byte `Value` (spec §7.1) |
| `src/heap.rs` | segmented word arena (`Vec<Box<[u64]>>`, 2^17-word segments, `seg << 17 \| word` refs), heap state, ICs, control stack |
| `src/rt.rs` | shared semantics: SmallInt fast paths + `#[cold]` slow paths, truthiness, global/attr/method ICs, list, dict, natives, fuel safepoint |
| `src/access.rs` | every code/frame/arena read and write; `--features unchecked` swaps in `get_unchecked` (the only `unsafe` in the crate) |
| `src/vm.rs` | `Program`, assembler, VM registers, frame push/pop, the `dispatchers!` macro that builds `run_match` and `run_fntable` from one handler list |
| `src/stack.rs` | design 1: stack machine (CPython/QuickJS style) |
| `src/acc.rs` | design 2: accumulator + registers (V8 Ignition style) |
| `src/reg3.rs` | design 3: three-address registers (Lua 5 style) |
| `src/kernels.rs` | kernel list, expected results, untimed setup |
| `kernels.py`, `kernels.js` | the same kernels for CPython 3.12 and QuickJS (dquickts) |
| `tools/handler_sizes.py` | machine-code size per variant from an unstripped build |
| `tools/summarize.py` | Markdown tables from `results/` |
| `run_all.sh` | runs everything below and writes `results/` |
| `results/` | raw outputs of the run reported in the results page |

## Reproduce

Prerequisites: Rust 1.99.0 (pinned by the repo's `rust-toolchain.toml`),
`~/.local/bin/python3.12`, a built QuickJS `qjs`:

```bash
git clone https://github.com/stencil-hq/dquickts /tmp/dquickts   # results used f9a3efaa
make -C /tmp/dquickts qjs
```

Everything, sequentially, pinned to CPU 2 (about 2 minutes):

```bash
cd spikes/dispatch
./run_all.sh            # env: REPS=15 PIN="taskset -c 2" PY=~/.local/bin/python3.12 QJS=/tmp/dquickts/qjs
```

Or step by step (what `run_all.sh` does):

```bash
export PATH=$HOME/.cargo/bin:$PATH
cd spikes/dispatch

# Builds: timed (default), bounds checks off, dispatch counter, unstripped.
cargo build --release
cargo build --release --features unchecked --target-dir target-unchecked
cargo build --release --features count --target-dir target-count
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --target-dir target-syms

# Wall time: 1 untimed verification round + 15 timed rounds; every round runs
# every (kernel, variant) once, interleaved. Prints `verify ...` lines, a CSV
# summary (min/median/max), a table and every raw sample.
taskset -c 2 ./target/release/dispatch bench --reps 15 > results/bench_checked.txt
taskset -c 2 ./target-unchecked/release/dispatch bench --reps 15 > results/bench_unchecked.txt
taskset -c 2 ./target/release/dispatch bench --reps 15 > results/bench_checked_run2.txt

# Bytecode bytes, static instruction counts, instructions dispatched.
./target-count/release/dispatch stats > results/stats.txt

# Machine code per variant (run_match; run_fntable + every handler in TABLE).
python3 tools/handler_sizes.py target-syms/release/dispatch > results/handler_sizes.csv

# Reference engines (each kernel: 1 untimed run + 15 timed, timer around the call only).
taskset -c 2 ~/.local/bin/python3.12 kernels.py 15 > results/cpython.txt
taskset -c 2 /tmp/dquickts/qjs kernels.js 15 > results/qjs.txt

# Hardware counters (unsupported in this VM: no PMU).
perf stat -e instructions,cycles,branch-misses ./target/release/dispatch bench --reps 1 --kernel fib --variant stack/match

python3 tools/summarize.py results > results/summary.md
```

Filters for quick looks: `dispatch bench --reps 3 --kernel loop --variant register3/match`.

## Checks

```bash
cargo build --release && ./target/release/dispatch bench --reps 1   # every line `verify ... OK`
cargo clippy --release -- -D warnings
cargo clippy --release --features unchecked -- -D warnings
cargo clippy --release --features count -- -D warnings
~/.local/bin/python3.12 kernels.py 1                                 # asserts EXPECTED
```

The default build has `#![forbid(unsafe_code)]`. `--features unchecked` exists
only to measure what bounds checks cost: it is unsound for untrusted bytecode.
`--features count` adds one counter increment per dispatch; never time it.
