# Isolation-mode harness

Native in-process, wasmtime in-process and child-process hosting of
micro-monty, measured with the same session code (`service/`) and workloads.
The write-up, the configuration choices and the results are in
[docs/research/isolation-modes.md](../../../docs/research/isolation-modes.md).

| crate / file | role |
|---|---|
| `service/` | one `Session` behind a request/response API, shared by all modes |
| `worker/` | child mode: the service over `u32 LE` length-prefixed frames on stdin/stdout |
| `guest/` | wasm mode: the service as a wasm32-wasip1 reactor (`mm_in`, `mm_call`) |
| `harness/` | drives the three modes: `perf` sections and single `fault` scenarios |
| [`crates/mm-wasm-abi`](../../../crates/mm-wasm-abi), [`crates/mm-wasm-guest`](../../../crates/mm-wasm-guest), [`crates/mm-wasm-host`](../../../crates/mm-wasm-host) | the supported wasm embedding, `mm-wasm` v1 ([docs/research/wasm-embedding.md](../../../docs/research/wasm-embedding.md)), used by path (it was prototyped here as `wasm/`): the codec, the guest (wasip1 and unknown-unknown, built in the root workspace by `run.sh`), the wasmtime reference host. Harness modes `abi` and `abi-uu` |
| `faults.py`, `report.py`, `run.sh`, `diff.py` | the fault matrix, the Markdown report, build-and-run, the diff of two fault matrices |
| `results/<date>/` | `perf.jsonl`, `faults.jsonl`, `report.md`, `build.txt`, `size.txt`; `2026-11-05-wasm-abi/` holds the runs before and after the robustness thread and `faults-diff.md` |

```bash
bash bench/evidence/isolation/run.sh [OUT_DIR] [perf|faults|all]
```

It runs on Linux and on macOS. On macOS the worker columns report
phys_footprint where Linux reports PSS, and the rlimit rows use `setrlimit`
in the child, because macOS has no `prlimit`
(`results/2026-11-05-m1max-aarch64-macos/README.md`).

This is a separate Cargo workspace with its own lockfile, building into
`target/isolation*`. Its dependencies (wasmtime, serde) never reach the
engine crates or the root build.
