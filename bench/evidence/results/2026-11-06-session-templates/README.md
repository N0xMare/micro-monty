# Session templates (2026-11-06)

[M] unless labelled. Engine: main at `1f27970`, unchanged; default stdlib
features everywhere (the `mm-codemode` `default` runner and the mm-wasm
guest share the build id). Analysis and recommendation:
`docs/research/session-templates.md`. Harnesses:
`bench/evidence/session_templates/` (`tpl`, `measure.py`, `codemode.py`,
`wasm/`) and the codemode runner's opt-in `MM_CODEMODE_TEMPLATE` and
`MM_CODEMODE_WARM`.

```
cargo build --release --manifest-path bench/evidence/session_templates/Cargo.toml --target-dir target/session-templates
cargo build --release --locked -p mm-codemode --target-dir target/codemode/default
python3.14 -I bench/evidence/session_templates/measure.py OUT              # templates-main.*
# codemode templates, captured with the runner's pinned clock:
TPL_PINNED=clock target/session-templates/release/tpl image SET IMG/SET.mmsnap   # SET = json, codemode, codemode-tz
python3.14 -I bench/evidence/session_templates/codemode.py OUT json=IMG/json.mmsnap \
    codemode=IMG/codemode.mmsnap codemode-tz=IMG/codemode-tz.mmsnap               # codemode-new-process.md, checks
python3.14 -I bench/evidence/session_templates/codemode.py OUT ... --skip-check --warm   # codemode-warm.md
tpl own conformance/own/CASE.py SET IMG/SET.mmsnap     # own-*.jsonl (one process per case)
tpl settings; tpl leak IMG/SET.mmsnap; tpl check OTHER_BUILD_IMAGE
# wasm: shipped guests (xtask's shipped_build flags, wasm-opt -Oz), precompiled:
taskset -c 2 target/session-templates-wasm/release/wasm-tpl GUEST.wasm IMG 21   # wasm.jsonl
```

| File | What |
|---|---|
| `templates-main.{md,json}` | Ir, wall, RSS and image sizes per set, fresh against restore |
| `templates-decode-cache-prototype.{md,json}` | The same with the decoded-module cache prototype |
| `decode-cache-prototype.patch` | The prototype (local branch `local/template-decode-cache`, not pushed) |
| `bench-ir-decode-cache.md` | Bench Ir, main against the prototype |
| `codemode-new-process.md`, `codemode-warm.md` | Codemode engine and setup percentiles per start |
| `codemode-timing-*.jsonl` | Per solution: setup and engine µs per start |
| `codemode-checks.jsonl` | Per solution: identical to fresh, per template |
| `own-codemode.jsonl`, `own-heavy.jsonl` | Own cases: fresh imports against the template |
| `settings.json` | Each host-side setting in an instance and in a control session |
| `leak-*.json` | Isolation, random state, clocks, hashing |
| `other-build.json` | Another build's image: `Incompatible` |
| `wasm.jsonl` | mm-wasm-host, both targets: fresh against restore |

Sizes with the prototype (`cargo xtask size`, against main): `hello.wasm`
2,078,951 → 2,079,750 B, native `hello` 2,276,888 → 2,278,264 B, mm-wasm
guest 2,182,840 → 2,183,641 B.
