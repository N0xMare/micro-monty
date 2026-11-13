# Base-relative snapshots

Commit hashes of micro-monty cited here, other than the public history's
(root `2bcbb8c`), are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`;
[docs/history](../history/README.md) maps them); the numbers they qualify
are unchanged.

Question (owner, 2026-11-05): can snapshots be stored relative to the
build's deterministic startup image (prelude, types and stdlib state as
`Session::new` leaves them), with the difference computed when the snapshot
is taken and no write tracking at run time? The goal is to make small parked
sessions much smaller.

Labels: [M] measured here, [S] sourced from code or earlier evidence, [E]
estimated. Raw results, commands and provenance:
`bench/evidence/results/2026-11-05-base-relative/`. Prototype:
`bench/evidence/base_relative/`. Engine `06f1fbc`, unchanged.

## Answer

**Yes, it works, but it does not make small parks much smaller, so it does
not belong in the engine.** The startup image is deterministic across
processes and targets [M], and in the median park of every group 91–99.6%
of its live heap is still byte-identical, at the same address [M]. But it is
small: 38,016 B of live heap in a 40,762 B image [M]. A median codemode park
is 99–140 KB, and only 24–36% of it is the base [M]. The rest is the
session's own heap (imported modules, tool wrappers, data) and the VM section
(about 20 KB [M]: code table, frames, tasks, globals [S spec §9.3]). The base-relative form is therefore 1.2–1.6×
smaller than FULL raw, and 1.2–1.4× smaller than FULL+zstd-1 [M].

The host can get the same gain without any engine change. zstd-1 with the
base image as a raw-content prefix (`--patch-from`) is within ±5% of the
block delta + zstd-1 on every small-park group [M]. A dictionary trained on
earlier parks does better again: 8.1× against FULL raw on held-out codemode
parks, where FULL+zstd-1 gives 4.0× [M].

**Recommendation:** keep FULL images as the engine format and add nothing to
the engine or to mm-wasm v1. On the host:

1. Compress at the storage boundary with zstd-1, the base image as a raw
   prefix, and the frame checksum on.
2. Key the stored blob by build id, which names the base.
3. Optionally add a dictionary trained on the host's own parks; store it with
   its id.

Revisit engine-side base-relative images only if a host cannot run zstd and
its sessions stay close to fresh. That is the one place the raw delta wins:
fib/loop/closure-style parks go 8.0–9.9× smaller raw [M].

## 1. How much of a FULL image is the fresh-session image

The base is `Session::new(&Engine::new(EngineConfig::default()))` then
`snapshot()`, on a build with `mm`'s stdlib features: 40,762 B, of which
38,016 B is live heap in two segments (one per space) [M]. "Same as base"
counts the 8-byte units of a park's live heap that are live in the base at
the same segment and offset, with the same bytes. Medians [M]:

| Images | n | FULL | heap same as base | share of FULL | VM section | segments identical |
|---|---:|---:|---:|---:|---:|---:|
| own cases, first park at `--every 100` | 117 | 79,504 | 36,200 | 45% | 22,374 | 0 / 240 |
| codemode, first park | 1,736 | 102,818 | 34,720 | 34% | 19,621 | 0 / 3,670 |
| codemode, every park | 12,991 | 98,983 | 36,032 | 36% | 20,244 | 0 / 27,739 |
| codemode, last park | 1,736 | 139,574 | 34,656 | 24% | 20,704 | 0 / 3,720 |
| bench, middle park of ~12 (12 programs) | 12 | 332,898 | 37,648 | 12% | 5,776 | 0 / 70 |
| bench fib / loop / closure, middle park | 3 | 41,491–42,312 | 37,752–37,880 | 89–91% | 3,156–3,873 | 0 / 6 |
| state, 1k rows | 1 | 178,860 | 37,720 | 21% | 3,941 | 0 / 2 |
| state, 100k rows | 1 | 13,891,552 | 37,720 | 0.3% | 3,957 | 0 / 14 |

- No segment is ever identical. New allocations land in the base's own
  segments, and a few base objects change (module tables, the intern table).
  A whole-segment comparison stores almost the full image (Δseg ≈ FULL + 52
  B).
- The base mostly survives: in the median park of each group, 34.7–37.9 KB
  of its 38.0 KB live heap is unchanged in place (91–99.6%).
- The premise that zstd's 3.6× on small parks means most of the image is the
  base does not hold. The base is 34–45% of a small park, so zstd's ratio
  comes mostly from redundancy inside the session's own data.
- Codemode parks are larger than the own cases. The runner defines `_ptc`
  with one async wrapper per tool [S `conformance/codemode/runner`], and
  990 of 1,793 solutions import `json` and 648 import `asyncio` [M
  `conformance/codemode/results/runs.jsonl`].

## 2. Prototype and measurements

`br` (`bench/evidence/base_relative/`, its own Cargo workspace linking the
unchanged facade) takes the FULL image from `Session::snapshot` and parses it
into header, segment tables, live runs and VM section
(`crates/mm-core/src/snapshot.rs` layout). It writes the following, block by
block against the base segment with the same index:

- magic and the base id (32 bytes of XXH64 over the base image, which embeds
  the build id);
- the header and the FULL image's checksum;
- every segment table and run list in full;
- for each segment, a bitmap of changed blocks and the changed blocks' live
  bytes, or all live data when the base has no such segment;
- the VM section in full.

`apply` rebuilds the exact FULL bytes and checks their XXH64 against the
stored checksum; then the unchanged `Engine::restore` reads them with its
full validation. Block sizes measured: 64 B, 256 B, 1 KiB, 4 KiB and whole
segment. No engine code changes, so run-time cost is zero by construction.

### Sizes

Medians in bytes [M] (all ratios and every group are in
`summary-startup-base.md`):

| Images | FULL | Δ 64 B | Δ 256 B | Δ 4 KiB | FULL+zstd1 | FULL+zstd1, base prefix | Δ 64 B+zstd1 |
|---|---:|---:|---:|---:|---:|---:|---:|
| own, first park | 79,504 | 51,888 | 59,447 | 64,373 | 22,333 | 15,509 | 15,887 |
| codemode, first park | 102,818 | 78,880 | 83,732 | 94,578 | 26,412 | 19,656 | 20,144 |
| codemode, every park | 98,983 | 71,086 | 77,550 | 86,835 | 23,582 | 16,531 | 17,161 |
| codemode, last park | 139,574 | 116,597 | 120,390 | 130,266 | 33,915 | 27,090 | 27,932 |
| bench fib, middle park | 42,232 | 4,795 | 5,439 | 18,909 | 8,415 | 885 | 1,298 |
| bench, middle park (12) | 332,898 | 300,760 | 305,030 | 317,800 | 36,387 | 28,641 | 31,315 |
| state, 100k rows | 13,891,552 | 13,859,258 | 13,860,138 | 13,883,490 | 714,118 | 708,205 | 708,173 |

Median per-image gain over FULL+zstd1 [M]:

| Images | base prefix | Δ 64 B+zstd1 |
|---|---:|---:|
| own, first park | 1.44× | 1.41× |
| codemode, first park | 1.34× | 1.31× |
| codemode, every park | 1.43× | 1.37× |
| bench fib / loop / closure | 9.5× / 11.0× / 7.4× | 6.5× / 7.2× / 5.6× |
| bench dict, json_rows, sort, str, fmt; state 10k, 100k | 1.00–1.08× | 1.00–1.10× |

Two variants did not help [M]:

- **Warm base.** A fresh session after `import json, asyncio, re,
  collections, datetime` (the five most common codemode imports) is 393,849
  B. Parks import in other orders, so fewer bytes line up: same-as-base falls
  from 34.7 to 22.1 KB on codemode first parks. Δ 64 B grows from 78.9 to
  83.2 KB and the zstd prefix from 19.7 to 23.9 KB (`summary-warm-base.md`).
- **Larger blocks.** 4 KiB blocks keep only 34–44% of the 64 B delta's
  saving on codemode parks; whole segments keep none.

A trained zstd dictionary does better than the startup base [M]
(`dictionary.md`). It was trained on 1,549 parks of sets `a`, `luna-a` and
`sol-a` and tested on the other sets (same tasks, other solutions). Ratios
are against FULL raw:

| Images | FULL+zstd1 | base prefix | dict 112 KiB | base + dict prefix |
|---|---:|---:|---:|---:|
| codemode held-out, first park (1,157) | 4.0× | 5.2× | 5.5× | 6.3× |
| codemode held-out, every park (8,345) | 4.0× | 5.6× | 7.5× | 8.1× |
| own cases, first park (117) | 3.6× | 4.8× | 4.7× | 5.0× |

### Save and restore time

One pinned core (`taskset -c 2`), one job. Each entry is engine + transform
+ zstd, in ms [M] (`timing.md`). Python's zstd binding times include loading
the prefix per call.

| Image | FULL | save FULL+zstd | save base prefix | save Δ64+zstd | restore FULL+zstd | restore base prefix | restore Δ64+zstd |
|---|---:|---:|---:|---:|---:|---:|---:|
| codemode median park | 104,191 | 0.07+0.27 | 0.07+0.28 | 0.07+0.06+0.21 | 0.13+0.42 | 0.13+0.42 | 0.11+0.03+0.42 |
| own median case | 79,504 | 0.08+0.21 | 0.08+0.22 | 0.08+0.04+0.15 | 0.10+0.16 | 0.08+0.16 | 0.07+0.02+0.16 |
| state, 100k rows | 13,891,552 | 16.1+16.2 | 16.1+16.1 | 16.1+8.5+16.4 | 15.0+8.9 | 12.0+8.9 | 11.7+7.5+8.9 |
| bench dict, middle | 17,267,664 | 25.7+26.0 | 25.7+26.2 | 25.7+13.3+26.6 | 18.2+14.2 | 17.3+14.2 | 18.0+11.7+14.2 |

Small parks cost the same in all three forms: about 0.35 ms to save and
0.55 ms to restore for a codemode park. On large heaps the delta adds 8–13
ms per save and 7–12 ms per restore and saves no bytes. An engine-native
version could fold the comparison into the write loop and avoid the parse
and the rebuilt copy [E].

### Behaviour

Every measured image rebuilt byte-identically from base + delta: 13,254
images × 5 block sizes [M]. Durability-style runs also restored every park
from base + delta and continued in the restored session [M]:

- **Programs:** 183 own cases run with a park every 100 fuel units (117
  reach one), 13 bench programs parked 12–14 times each (`startup` finishes
  before its first park), and 3 state programs. All 199 produced the same
  output and result as a plain run, over 89,875 restores. All 13 bench
  checksum lines matched.
- **Codemode:** all 1,793 solutions, with a park at every host call and wait
  (12,991 parks). Both the measured run and the swap run equalled the
  unchanged `mm-codemode` runner in outcome, error, checker verdict, tool
  calls, batches and stdout.
- **After the final prototype edits** (a lookup table instead of a hash map,
  and a sorted-run search): a rerun of 58 own cases and 153 codemode
  solutions matched again, and 32 of 32 overlapping images kept identical
  sizes [M].

Rejection with the wrong base [M] (`README.md` in the results):

| Wrong base | base id check | id forced, rebuilt-image checksum | no rebuild check, `Engine::restore` | no checks at all |
|---|---|---|---|---|
| warm base (same build) | refused | refused | `Corrupt { checksum }` | `Corrupt { heap }` |
| default-feature build's base | refused | refused | `Corrupt { checksum }` | **restores, into a different session** |

The engine's restore validation does not detect a base mix-up: the delta
carries the target's build id, and a wrong base can still yield a valid heap.
A base-relative format therefore needs both checks, the base id and an
end-to-end checksum of the rebuilt image.

The zstd prefix route fails safe without extra work. The decompressed bytes
are a whole FULL image whose XXH64 trailer `Engine::restore` checks first
[S `crates/micro-monty/src/lib.rs` `restore_unguarded`]. A wrong prefix
decoded to different bytes (33,432 of 50,169) when zstd's checksum flag was
off; with the flag on, zstd refused [M].

## 3. Cost and risk

**Determinism** [M]. The fresh-session image was byte-identical in every
case tried:

- full-feature build: 7 native x86-64 processes (two with `env -i
  TZ=Asia/Tokyo`) and `wasm32-wasip1` under wasmtime, all 40,762 B, SHA-256
  `06f4c530…`;
- default-feature build: the native facade, the mm-wasm v1 guest on
  `wasm32-unknown-unknown` (3 instances), the guest on `wasm32-wasip1` and the
  guest after `wasm-opt -Oz`, all 40,702 B, `ef009a06…`.

So 64-bit native and 32-bit wasm agree, and `Session::new` draws no entropy,
clock or environment into the image. Not measured: native aarch64 (no
machine here). Its value encoding is the same little-endian fixed width and
the build id is target-independent [S `build_id`], so equality is expected
[E].

The risk is future drift. Any change that makes `Session::new` depend on
the process would silently invalidate stored bases. Examples are seeding
`random` from entropy at startup, caching a hash with a per-process seed, or
target-dependent allocation sizes. Deltas would then be refused (safe), and
every stored delta would become unreadable. An engine format would need a
gate test that compares base images across native and wasm.

**What the host stores.** One base per build id and feature set: 40.7–40.8
KB [M]. A host can also regenerate it with `Session::new` + `snapshot`, as
long as determinism holds. That costs one session creation; all of `mm run
bench/startup.py` is about 7.1M instructions [S, docs/history/handoff-0.0.1.md]. A trained dictionary adds 64–112 KiB
per build [M]. Every stored image must record the base or dictionary id;
zstd frames with a raw prefix carry no dictionary id.

**Snapshot validation.** The prototype keeps restore validation unchanged by
rebuilding the FULL image first. An engine-native form that patched segments
in place would add new reader surface: block bitmaps, block ranges against
segment `top`, and runs against blocks. All of it would have to be
bounds-checked before the existing checks and added to the snapshot fuzz
target (spec §9.3). The base also stays trusted input, like snapshots.

**mm-wasm v1 (frozen, `5568b27`).** Only additive optional exports are
allowed. An engine-side form would be two optional exports (relative
snapshot and relative restore, base held or regenerated inside the guest).
Measured on the guest before the freeze, the prototype transform with those
two exports adds 22,475 B after `wasm-opt -Oz` (2,223,853 → 2,246,328; +8,487
gzipped) [M]. The frozen guest is 2,230,840 B against a 2,235,000 ceiling
[S], so the exports would breach the ceiling by about 18 KB [E]. An
engine-native rewrite would be smaller (a compare loop in
`Space::write_snapshot` and a patch step before `read_snapshot`'s checks),
roughly 5–10 KB [E]. The host-side zstd route needs no export: `mm_snapshot`
already returns the FULL bytes.

**Size and Ir of an engine change.**

- Run-time instructions: zero by construction in either design, because no
  code runs on the write path or in `mm run`. The bench Ir would equal
  main's [E]; the prototype changes no engine code.
- Native `hello` is unchanged unless it calls the API [E].
- Snapshot-time cost: encode 0.06 ms (codemode median) to 8.5 ms (100k rows)
  and 13.3 ms (17 MB dict); apply 0.03 to 7.5 and 11.7 ms [M, prototype].
- Wasm guest: +22.5 KB measured for the prototype [M], about 5–10 KB for a
  native rewrite [E].

**Compared with the rejected design.** Write tracking cost 4–19% execution
time [S `bench/evidence/results/`]. Snapshot-time comparison costs nothing
at run time and 0.1 ms per small snapshot. Its limit is the size of the
shared base, not CPU.
