# Spike S2: execution core redesign

Throwaway prototype of a redesigned micro-monty execution core, built to
decide whether it can reach about 1.2× QuickJS wall time on `fib` and
`closure` before anyone ports it into `mm-engine`. Results and method:
[`docs/spikes/dispatch2.md`](../../docs/spikes/dispatch2.md). Nothing here
ships. Like S1 (`spikes/dispatch`), the package is its own cargo workspace,
and the root `Cargo.toml` excludes `spikes/`. `#![forbid(unsafe_code)]`.

| File | What |
|---|---|
| `src/value.rs` | S1's NaN-boxed `Value`, plus the frame link word |
| `src/heap.rs` | segmented checked arena, functions with inline cells, cells, interned str, dicts (index + entries), range iterators, exceptions, precise mark from roots |
| `src/code.rs` | 32-bit instruction words, opcodes, the flat code table (`CodeEntry`), IC entries, assembler |
| `src/vm.rs` | the core: `Vm::run` (one dispatch loop), `cold_op` (one rare instruction, out of line), safepoint/fuel/GC, unwinding, slow paths |
| `src/snapshot.rs` | save/restore of the whole execution state |
| `src/kernels.rs` | the bench programs, hand-assembled as mm-engine's compiler emits them, plus semantic test programs |
| `src/main.rs` | `run <kernel> [reps]`, `test`, `stats` (with `--features count`) |
| `programs/` | `callfn`, `loopfn`, `dictint`, `empty` in Python and JS for mm and qjs (`fib`, `closure`, `loop` are `bench/`) |
| `tools/callgrind.sh` | instruction counts for spike, mm and qjs |
| `tools/wall.py` | pinned, interleaved whole-process wall medians |
| `tools/stability.sh` | instruction counts under unrelated edits (`perturb_*` features) |
| `tools/hot.py` | per-instruction callgrind costs joined with `objdump` for one function |
| `tools/summarize.py` | the tables in `results/summary.md` |
| `run_all.sh` | everything, in order |

## Reproduce

Prerequisites: Rust 1.99.0 (the repo's `rust-toolchain.toml`), valgrind,
and a QuickJS `qjs`:

```bash
sudo apt-get install -y valgrind
git clone https://github.com/stencil-hq/dquickts /tmp/dquickts && git -C /tmp/dquickts checkout b51e92c
make -C /tmp/dquickts qjs
cd spikes/dispatch2 && ./run_all.sh     # env: QJS=/tmp/dquickts/qjs MM=../../target/release/mm CPU=2
```

## Checks

```bash
cargo build --release && ./target/release/dispatch2 test     # every line `ok`
cargo clippy --release -- -D warnings
cargo clippy --release --no-default-features -- -D warnings
cargo clippy --release --features count -- -D warnings
```

`test` checks every kernel's result and its exact fuel ticks. It also
parks each kernel every 1, 7, 4,999 and 100,003 ticks (the first 3,000 or
1,000 parks for 1 and 7), and at each park snapshots the VM, drops it,
builds a new VM from the program and the snapshot, and continues. One run
also collects garbage at every safepoint. The remaining checks are a
caught RecursionError at the limit, the generic range path (iterator
object), and an uncaught slow-path error three frames down.
