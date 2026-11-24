# micro-monty

micro-monty is a small, fast, durable Python 3.14 subset engine in Rust,
built for code mode: LLM agents that call tools by writing Python cells.
It has no ambient I/O; files, network, clocks and randomness reach Python
only through functions the host provides. A running cell can stop at any
safepoint, at any call depth, and continue later, in the same process or
from a snapshot in another process or on another target. Memory, fuel,
recursion and output are bounded per session.

It is inspired by [Monty](https://github.com/pydantic/monty), Pydantic's
Python interpreter in Rust for the same job, and runs Monty's test corpus
as part of its conformance gate.

## Size and specs

Measured on v0.0.3 (the release commit, on `0c970b8` and its landing) [M]:

- `hello.wasm` 2,259,536 B, native `hello` 2,451,824 B (stripped, the whole
  engine and its default stdlib).
- About 95 KB per set-up session, and 0.10 ms for a new session plus setup
  (measured on v0.0.1; not re-measured since).
- Durable snapshots: a cell parked anywhere restores in a new process,
  natively or on either wasm target, on a build with the same build id.
- 100% of the model-written code-mode solutions that pass on CPython pass
  on micro-monty (1,722 of 1,722, `conformance/codemode`).

## Platforms

Tested on Linux x86_64, with glibc and with musl (`x86_64-unknown-linux-musl`,
static), and macOS on Apple Silicon; every other platform is untested. musl
runs every case and recorded session as glibc does, but slower: its `malloc`,
`memcpy` and `memcmp` add 54% to recorded agent sessions' engine time in
memory and 82% with a snapshot and restore at every host call [M, 0.0.3];
a global allocator such as mimalloc in the host binary recovers half to
three quarters of it [M]. This repository's `mm` and `mm-cell-worker`
register mimalloc on musl; building them there needs `musl-gcc` (Debian's
`musl-tools`), and mimalloc is compiled with `-mpopcnt`, so the musl
binaries need a CPU with POPCNT (every x86-64 CPU since 2009), which the
Rust target's baseline does not ask for. The step bound (docs/spec.md §5)
holds there on every `Bounded` row of the step-time oracle, which the
release gate runs on musl twice, with musl's `malloc` and with mimalloc.
Two rows are known to exceed the limit on some runs with mimalloc and are
not judged there (they are with glibc and with musl's `malloc`):
`json.dumps(d, sort_keys=True, indent=1)` of a million keys, 53.1 to
65.4 ms against 52.5 to 52.9 in 8 of 8 runs, and a dict of 2^20 colliding
keys filled beside a 46 MiB bytearray, 36.9 to 61.3 ms against 52.5 to
53.2 in 4 of 8 [M]. The wasm
targets `wasm32-wasip1` and `wasm32-unknown-unknown` are supported builds,
gated with the native ones (the facade, the mm-wasm guest under wasmtime,
and snapshots restored across targets).

## Using it

From crates.io, as a library (`cargo add micro-monty`):

```toml
[dependencies]
micro-monty = "0.0.3"
```

The [user guide](docs/user-guide.md) is the crate's documentation: embedding,
host functions, limits, snapshots, the `mm` CLI and wasm hosts.

The `mm` CLI and the `mm-cell-worker` binary (a worker process for the cell
protocol of [plak](https://github.com/N0xMare/plak), an open-source agent
harness; [spec §10.2](docs/spec.md)) are attached to each
[GitHub release](https://github.com/N0xMare/micro-monty/releases) for Linux
and macOS, x86_64 and arm64 each (only the two platforms above are tested),
with SHA256SUMS; releases after v0.0.1 also attach the two mm-wasm guests. From source, with the toolchain pinned in
`rust-toolchain.toml`:

```bash
cargo build --release -p mm-cli --bin mm
target/release/mm run program.py
```

How the engine works is in [docs/spec.md](docs/spec.md); changes per release
are in [CHANGELOG.md](CHANGELOG.md).

## License

`MIT AND PSF-2.0 AND Unicode-3.0`: micro-monty's own code is MIT, and the
crates include code ported from CPython (PSF License) and Unicode data
(Unicode License v3). [THIRD_PARTY.md](THIRD_PARTY.md) lists each, with its
notice; every published crate carries it and LICENSE.
