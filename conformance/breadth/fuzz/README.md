# Fuzz artifacts

`artifacts/` holds the inputs the libFuzzer targets in `fuzz/` found
(crashes, slow units, out-of-memory cases), most of them minimized, and
`gen-c4-straightline.py`, which generates one of them. The tests in
`crates/micro-monty/tests/session.rs` and `snapshots.rs` replay them, so they
stay in the tree; each file's first byte selects what its target does.

```bash
cd fuzz
cargo +nightly-2026-11-01 fuzz build            # every target, AddressSanitizer
cargo +nightly-2026-11-01 fuzz run <target> -- -max_total_time=1200 -timeout=60 -print_final_stats=1
cargo +nightly-2026-11-01 fuzz tmin <target> <artifact> -r 50 -- -timeout=5
target/x86_64-unknown-linux-gnu/release/<target> ../conformance/breadth/fuzz/artifacts/<file>
```

nightly-2026-11-01 and cargo-fuzz 0.13.2 come from `.agents/setup`. The
campaign reports (2026-11-04 to 2026-11-10) are in git history before the
docs were cut to `docs/spec.md` and `docs/user-guide.md`.
