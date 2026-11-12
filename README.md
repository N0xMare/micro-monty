# micro-monty

Small, fast, durable Python-subset sandbox engine in Rust.

micro-monty runs the Python that LLM agents write, with no ambient I/O: every
effect is a host call. Its defining property is pause-anywhere: a running cell
can stop at any safepoint, at any call depth, and continue later, in the same
process or from a snapshot in another one.

Status: pre-release (0.0.x). Numbers are [M] measured, at the commits and in
the documents named:

- **Correctness:** 546 of the 568 CPython-scored cases of Monty's test
  corpus pass (`conformance/results/micro-monty.md`), and every passing case
  gives the same result when parked and restored in a new process at every
  few fuel units (`cargo xtask durability`).
- **Real code-mode sessions:** 2,841 recorded cells from 160 agent sessions
  replay with their recorded outcomes (`conformance/codemode/real`).
  On eight review tasks written as Python cells on mm and as JavaScript on
  a QuickJS host, two models answered and graded the same in both, with
  a median engine time per cell of 1.5–3.2 ms on mm against 4.1–9.2 ms
  ([docs/research/js-vs-python-models.md](docs/research/js-vs-python-models.md),
  first sample, orb timings).
- **Against the code-mode JavaScript engines** (at 5df122e,
  [docs/research/js-engines-compare.md](docs/research/js-engines-compare.md)):
  131 KB of RSS and 0.15 ms per new session after setup, 3.3 µs per awaited
  host call, and 23.5 ms for 22 code-mode tasks end to end against
  120–346 ms. It loses on size (the native build is 1.9× rquickjs's) and on
  raw interpretation (native QuickJS is 1.3× faster on the bench geomean).

- Embedding it in Rust: [docs/sdk.md](docs/sdk.md), the SDK guide.
- The design, measurements and milestones: [docs/spec.md](docs/spec.md).

## Building

```bash
cargo run --release -p mm-cli --bin mm -- run program.py
cargo xtask check    # fmt, clippy, tests, docs, packaging, parse-diff, size, embedding workload, step-time oracle, golden snapshots, memory-bound probes, shipped own cases, wasm, conformance (native and wasip1), session replay, durability, bench checksums
```

The toolchain is pinned in `rust-toolchain.toml`. Changes per release:
[CHANGELOG.md](CHANGELOG.md).

## Platform support

- **Tested:** x86_64 Linux (every gate of `cargo xtask check`), aarch64
  macOS (every gate as of ac10b28, 2026-11-05; the gates added since have
  no recorded macOS run), `wasm32-wasip1` and `wasm32-unknown-unknown` (the facade and the
  mm-wasm guest under wasmtime, snapshots restored across them and native).
- **Never run:** Windows. Nothing in the engine is platform-specific, but
  the tooling is Unix-only (`.agents/setup`, `tar` in `cargo xtask
  package`), the `mm-cell-worker` binary's seccomp filter is Linux-only,
  and the published crates' LICENSE and THIRD_PARTY.md are symlinks in the
  repository.
- **Big-endian:** no big-endian host has run the gates. Snapshot images
  are endian-independent by construction: every integer is written and
  read explicitly little-endian, floats as their IEEE bits and strings as
  little-endian words, no code converts with the native byte order, and
  `sys.byteorder` and `struct`'s native mode are little-endian on every
  target. Under Miri on `s390x-unknown-linux-gnu`, images are
  byte-identical to x86_64's and restore across the two [M] (on d2fa244;
  not rerun since).

## License

`MIT AND PSF-2.0 AND Unicode-3.0`: micro-monty's own code is MIT, and the
crates include code ported from CPython (PSF License) and Unicode data
(Unicode License v3). [THIRD_PARTY.md](THIRD_PARTY.md) lists each, with its
notice; every published crate carries it and LICENSE.
