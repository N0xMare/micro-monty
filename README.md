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

Measured on v0.0.1 (`1cfcde3`) [M]:

- `hello.wasm` 2,271,969 B, native `hello` 2,490,336 B (stripped, the whole
  engine and its default stdlib).
- About 95 KB per set-up session, and 0.10 ms for a new session plus setup.
- Durable snapshots: a cell parked anywhere restores in a new process,
  natively or on either wasm target, on a build with the same build id.
- 98.8% of the model-written code-mode solutions that pass on CPython pass
  on micro-monty (1,701 of 1,722, `conformance/codemode`).

## Platforms

Tested on Linux x86_64 and macOS on Apple Silicon; every other platform is
untested. The wasm targets `wasm32-wasip1` and `wasm32-unknown-unknown` are
supported builds, gated with the native ones (the facade, the mm-wasm guest
under wasmtime, and snapshots restored across targets).

## Using it

From crates.io, as a library (`cargo add micro-monty`):

```toml
[dependencies]
micro-monty = "0.0.1"
```

The [user guide](docs/user-guide.md) is the crate's documentation: embedding,
host functions, limits, snapshots, the `mm` CLI and wasm hosts.

The `mm` CLI and the `mm-cell-worker` binary are attached to each
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
