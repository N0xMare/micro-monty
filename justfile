# Wrappers over the cargo and xtask commands AGENTS.md describes; each runs
# exactly what it names. `just --list` shows them all.

# The previous release tag, the base of the pre-tag drift checks.
prev_tag := `git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || echo none`

# Build the workspace.
build:
    cargo build --workspace --locked

# The workspace's tests (debug).
test *args:
    cargo test --workspace --locked {{args}}

# Format every crate.
fmt:
    cargo fmt --all

# rustfmt check and clippy with warnings denied.
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings

# The landing tier: every gate, before a push to main.
check:
    cargo xtask check

# The release gate: before a tag, and for the changes AGENTS.md names.
release-gate:
    cargo xtask check --release-gate

# Monty's corpus on mm against conformance/results (`--filter X` runs a subset).
conformance *args:
    cargo xtask conformance --engine micro-monty {{ if args == "" { "--check" } else { args } }}

# Every passing case parked, snapshotted and restored: reports, fuel and
# final-state digests against conformance/durability-results.txt.
durability *args:
    cargo xtask durability {{args}}

# The facade on both wasm targets, with the round trip under wasmtime.
wasm:
    cargo xtask wasm

# The shipped builds against the base's and the release's (Size: trailers,
# xtask/size-budget.txt).
size:
    cargo xtask size

# Bench checksums on CPython and mm (pass `--reps N` and engines for timings).
bench *args:
    cargo xtask bench {{ if args == "" { "--quick --engines cpython,micro-monty" } else { args } }}

# The pre-tag checklist on the tree to tag, against the previous tag.
pretag base=prev_tag:
    cargo xtask check --release-gate
    cargo check --manifest-path fuzz/Cargo.toml --features memory-probes --bins
    cargo deny --all-features check
    cargo deny --all-features --manifest-path fuzz/Cargo.toml check
    cargo deny --all-features --manifest-path crates/mm-wasm-host/Cargo.toml check
    {{ if base == "none" { "true" } else { "cargo xtask wasm-ir --base " + base } }}
    {{ if base == "none" { "true" } else { "cargo xtask native-ir --base " + base } }}
    cargo xtask native-ir --plain
    {{ if base == "none" { "true" } else { "cargo xtask v8-ir --base " + base } }}
