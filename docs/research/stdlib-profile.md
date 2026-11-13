# A `codemode-core` stdlib profile: cost against use

Commit hashes of micro-monty cited here, other than the public history's
(root `2bcbb8c`), are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`;
[docs/history](../history/README.md) maps them); the numbers they qualify
are unchanged.

Measured 2026-11-08 on `e96ca7c`. Rust 1.99.0, binaryen 133, x86-64 orb.
Labels: [M] measured, [E] estimated. Raw numbers, commands and scripts:
`bench/evidence/results/2026-11-08-stdlib-profile/` and
`bench/evidence/stdlib_profile/`.

**Follow-up** (owner decision 62, docs/HANDOFF.md): no profile feature
ships. docs/sdk.md ("Smaller module sets for code-mode agents") gives
core35 and real-min as feature lists, without `mod-math`, which gated
nothing (`math` is always built) and was dropped. The undeclared imports
below, and a tenth
(`collections.abc` → `abc`), are now in the feature lists, and
mm-stdlib's build script fails on any new one. real-min therefore brings
`abc` with `collections`. The numbers below are the study's, at e96ca7c.

## Summary

- **A codemode profile saves about 4% [M].** `core35` keeps every module
  that at least 2% of either corpus uses, every module a passing real
  cell needs, and three small ones (`textwrap`, `hmac`, `zlib`). It cuts
  hello.wasm from 2,082,251 to
  1,996,756 bytes (−85,495, −4.1%), native `hello` by 89,472 (−3.9%) and
  the guest by 85,260 (−3.9%). Under it, no real plak cell that passes today
  fails, and 42 of the 1,782 synthetic solutions (2.4%) go from pass to an
  import error (zoneinfo 21, calendar 9, tomllib 9, shlex 3).
- **The plak-only floor saves 10% but fails 31% of the synthetic set
  [M].** The smallest set that still passes every real cell (20 modules)
  saves 212,796 B of hello.wasm and fails 549 synthetic solutions. The
  code-review sessions use `json`, `asyncio` and `collections` and little
  else. The synthetic data tasks also use `datetime`, `csv`, `statistics`,
  `decimal`, `html` and `urllib.parse`.
- **Recommendation: do not add `stdlib-codemode-core` yet.** The 85 KB
  is spread over 20 modules, 13 of which neither corpus uses
  (43,569 B of it). It changes no run-time cost, and a second shipped
  configuration has to be tested for build ids, durability and the
  wasm host. What the measurement found that needs fixing is in the
  feature graph:
  **eight imports that run when a module is imported are missing from
  the `mod-*` dependency lists**, so a hand-written
  `default-features = false` list that the manifests accept can fail
  at import (`from typing import Any` raises `ModuleNotFoundError: No
  module named 'functools'`). Fix those first. If the profile ships
  anyway, its contents are `core35` below.
- **Bench and import Ir do not change [M].** Every bench program is
  within 0.02% under `core35`, and an empty session's first Ir is
  −0.20%. Importing a module costs the same in either build (±0.7%).
- **The default build's stdlib is 469,122 B of hello.wasm [M]** (22.5%).
  The build with no stdlib module now compiles, so size-map.md's caveat
  that it fails without `struct` no longer holds. `re` and its 17
  importers are 192,741 B of that.

## Method

**Sizes** (`removals.py`). Each build is what `cargo xtask size`
ships: `RUSTC_BOOTSTRAP=1`, `-Zbuild-std=std,panic_abort`,
`-Cpanic=immediate-abort -Cforce-unwind-tables=no`, release, `--locked`;
`hello` as `cargo rustc --example hello -- -Copt-level=3` for
wasm32-unknown-unknown and the host, the guest with `cargo build -p
mm-wasm-guest`, both wasm modules through `wasm-opt -Oz` with the shipped
feature list. Builds run in git worktrees of HEAD whose paths have the
same length as the checkout, and only the facade's `default` list
changes, never the feature graph. The worktree baseline reproduces
`cargo xtask size` to the byte: 2,082,251 / 2,265,576 / 2,166,578 [M].
That differs from size-map.md's method, which also deleted the dropped
feature from its importers' lists and so measured a module's own bytes
in builds whose importers were broken. Here each number is a build that
works. Where a module cannot leave alone, the number is for the module
plus every default module that imports it (the group column).

**Usage** (`census.py`). Imports come from each source's AST (CPython
3.14, top-level await allowed), at any depth; a source that does not
parse imports nothing.
- Real: every `start_cell` of `conformance/codemode/real/traces.tar.xz`,
  2,841 cells in 160 sessions. A cell also needs what its session's
  bootstrap imports. plak's py-sdk bootstrap imports `asyncio` and
  `collections`, so 1,615 cells use `collections` without importing
  it.
- Synthetic: the 1,782 census solutions in
  `conformance/codemode/solutions`. The directory holds 1,793 files; the
  11 hand-written probes in `p/` are left out, as in every census count.
  The request's 1,820 matches no count in the repository.
- A unit *uses* a module if it imports it or a module that imports it at
  import time. 36 real cells do not parse; 4 import `subprocess` and 3
  `ast`, which the default build does not have; 20 synthetic solutions
  import `xml` and 9 `email`.

**Behaviour** (`behaviour.py`). For each set, `mm-cell-worker`'s
`replay_worker` and `mm-codemode` are built with that module list and run
over both corpora with the gates' own code (`gate.replay_session`,
`run.run_all`). Each cell and solution is compared with the committed
expectation of the default build (`results/gate.jsonl`,
`results/runs.jsonl` `mm-default`). The control, an unchanged build in a
worktree, differs on 0 of 2,841 cells and 0 of 1,782 solutions [M]. So
every difference under a set comes from the set. "Fails" below means a
real cell that completes on the default build and fails under the set,
or a synthetic solution that passes and stops passing. The static census
(a unit imports a module outside the set) is given beside it.

**Ir.** `bench/callgrind.py` on two `mm` binaries from worktrees of equal
path length. Both have `mm-cli`'s opt-in modules removed, so the binaries
differ only in the module set. Import Ir comes from
`bench/evidence/import_cost` (`imp`), linked against each module list.

## The feature graph imports more than it declares

`features.py --check` lists every import in the bundled Python that the
module's feature does not declare, directly or through other features.
Eight run at import time, outside any `try` or `TYPE_CHECKING` guard:

| module | imports at import time, undeclared |
|---|---|
| `typing` | `collections`, `functools` |
| `functools` | `collections` (`namedtuple`) |
| `random` | `itertools`, `operator` |
| `datetime`, `time`, `os`, `hashlib` | `operator` (`index`) |

The measurement found them: a set with every module the real cells
import, closed over the declared features (`profile:real (declared
features only)` in the evidence), failed 10 passing real cells. 9 failed
on `from typing import Any` ("No module named 'functools'") and 1 on
`import random` ("No module named 'itertools'") [M]. The default build
has all of them, so only custom lists are affected. This study follows
these edges everywhere, so every measured set imports cleanly. Other
imports happen only inside functions, so they fail on use:

- `collections.abc`, imported by `ChainMap` and `UserDict` views and by
  subclassing a `typing` alias, needs `abc`;
- `time.strptime` needs `datetime`;
- `re`'s and `traceback`'s deprecation paths need `warnings`;
- `shlex.split(..., posix)` paths need `os`;
- `operator.methodcaller` needs `functools`.

`mod-math` gates nothing: `math` is an engine module that is always
built (mm-engine `modules::BUNDLED`). Its row below is the bytes of
`decimal` and `urllib`, which list it.

## Cost against use

Bytes saved by removing the module (and, where it cannot leave alone, the
listed modules that import it) from the default set [M]. Uses are units
that import the module or a module importing it: real cells of 2,841,
synthetic solutions of 1,782 [M]. B/use is hello.wasm bytes over real
plus synthetic uses; the table is sorted by it, worst first.

| module | hello.wasm | native hello | guest | real uses | synthetic uses | removed with it (imports it) | B/use |
|---|---:|---:|---:|---:|---:|---|---:|
| copy | −15,139 | −15,424 | −15,116 | 0 | 0 | dataclasses, pprint | ∞ |
| traceback | −13,497 | −13,680 | −13,482 | 0 | 0 | logging | ∞ |
| dataclasses | −12,961 | −13,136 | −12,946 | 0 | 0 | pprint | ∞ |
| string | −9,287 | −9,472 | −9,272 | 0 | 0 | logging | ∞ |
| logging | −7,796 | −7,888 | −7,788 | 0 | 0 | alone | ∞ |
| difflib | −5,236 | −5,328 | −5,228 | 0 | 0 | alone | ∞ |
| pprint | −5,132 | −5,232 | −5,124 | 0 | 0 | alone | ∞ |
| contextlib | −4,052 | −4,144 | −4,044 | 0 | 0 | alone | ∞ |
| codecs | −2,484 | −2,560 | −2,476 | 0 | 0 | alone | ∞ |
| platform | −780 | −880 | −772 | 0 | 0 | alone | ∞ |
| posixpath | −564 | −656 | −556 | 0 | 0 | alone | ∞ |
| gc | −556 | −656 | −548 | 0 | 0 | alone | ∞ |
| tempfile | −548 | −656 | −540 | 0 | 0 | alone | ∞ |
| types | −19,745 | −20,416 | −19,692 | 0 | 9 (0.5%) | difflib, pprint, tomllib | 2,194 |
| tokenize | −6,451 | −8,080 | −6,416 | 4 (0.1%) | 0 | alone | 1,613 |
| shlex | −2,756 | −2,864 | −2,748 | 0 | 3 (0.2%) | alone | 919 |
| tomllib | −7,585 | −7,968 | −7,554 | 0 | 9 (0.5%) | alone | 843 |
| typing | −13,763 | −14,208 | −13,724 | 9 (0.3%) | 9 (0.5%) | tomllib | 765 |
| struct | −21,917 | −25,040 | −21,862 | 3 (0.1%) | 26 (1.5%) | zoneinfo | 756 |
| enum | −23,017 | −23,568 | −22,970 | 0 | 35 (2.0%) | calendar, zoneinfo | 658 |
| abc | −35,868 | −36,240 | −35,837 | 0 | 61 (3.4%) | contextlib, decimal, numbers | 588 |
| contextvars | −30,602 | −30,784 | −30,587 | 0 | 61 (3.4%) | decimal | 502 |
| numbers | −30,378 | −30,544 | −30,363 | 0 | 61 (3.4%) | decimal | 498 |
| decimal | −28,919 | −29,024 | −28,911 | 0 | 61 (3.4%) | alone | 474 |
| calendar | −14,515 | −14,976 | −14,476 | 0 | 35 (2.0%) | zoneinfo | 415 |
| itertools | −72,967 | −74,592 | −72,878 | 1 (0.0%) | 176 (9.9%) | calendar, decimal, random, statistics, zoneinfo | 412 |
| operator | −163,507 | −175,376 | −163,226 | 39 (1.4%) | 358 (20.1%) | calendar, datetime, fnmatch, fractions, hashlib, hmac, logging, os, pathlib, posixpath, random, statistics, struct, time, tomllib, zoneinfo | 412 |
| hashlib | −50,535 | −53,344 | −50,429 | 1 (0.0%) | 131 (7.4%) | hmac, random, statistics | 383 |
| os | −21,929 | −22,752 | −21,858 | 36 (1.3%) | 26 (1.5%) | fnmatch, pathlib, posixpath, zoneinfo | 354 |
| time | −76,705 | −82,368 | −76,602 | 0 | 227 (12.7%) | calendar, datetime, logging, tomllib, zoneinfo | 338 |
| zoneinfo | −8,369 | −8,752 | −8,338 | 0 | 26 (1.5%) | alone | 322 |
| bisect | −33,060 | −34,496 | −32,987 | 1 (0.0%) | 103 (5.8%) | random, statistics, zoneinfo | 318 |
| fnmatch | −5,583 | −5,776 | −5,568 | 18 (0.6%) | 0 | pathlib | 310 |
| random | −23,852 | −24,800 | −23,819 | 1 (0.0%) | 77 (4.3%) | statistics | 306 |
| warnings | −66,593 | −72,240 | −66,490 | 0 | 236 (13.2%) | calendar, datetime, hmac, tomllib, zoneinfo | 282 |
| datetime | −61,777 | −67,264 | −61,690 | 0 | 227 (12.7%) | calendar, tomllib, zoneinfo | 272 |
| functools | −107,401 | −113,712 | −107,242 | 9 (0.3%) | 393 (22.1%) | calendar, contextlib, datetime, fractions, hmac, statistics, tomllib, typing, urllib, warnings, zoneinfo | 267 |
| re | −192,741 | −203,104 | −192,491 | 45 (1.6%) | 699 (39.2%) | calendar, datetime, decimal, difflib, fnmatch, fractions, hmac, html, logging, pathlib, pprint, statistics, textwrap, tomllib, urllib, warnings, zoneinfo | 259 |
| pathlib | −4,068 | −4,176 | −4,060 | 18 (0.6%) | 0 | alone | 226 |
| fractions | −16,921 | −17,104 | −16,906 | 0 | 77 (4.3%) | statistics | 220 |
| textwrap | −2,532 | −2,640 | −2,524 | 3 (0.1%) | 9 (0.5%) | alone | 211 |
| math (gates nothing) | −37,212 | −37,488 | −37,189 | 4 (0.1%) | 184 (10.3%) | decimal, urllib | 198 |
| binascii | −15,770 | −16,224 | −15,737 | 29 (1.0%) | 54 (3.0%) | base64, zlib | 190 |
| hmac | −1,468 | −1,552 | −1,460 | 0 | 9 (0.5%) | alone | 163 |
| statistics | −10,430 | −10,528 | −10,420 | 0 | 77 (4.3%) | alone | 135 |
| io | −18,115 | −18,784 | −18,060 | 5 (0.2%) | 138 (7.7%) | pprint, shlex, zoneinfo | 127 |
| urllib | −8,343 | −8,528 | −8,328 | 0 | 80 (4.5%) | alone | 104 |
| html | −7,569 | −7,856 | −7,546 | 0 | 79 (4.4%) | alone | 96 |
| heapq | −175,125 | −183,680 | −174,878 | 1,627 (57.3%) | 638 (35.8%) | calendar, collections, contextlib, datetime, decimal, difflib, fractions, functools, hmac, pprint, shlex, statistics, tokenize, tomllib, typing, urllib, warnings, zoneinfo | 77 |
| collections | −173,549 | −182,016 | −173,310 | 1,627 (57.3%) | 638 (35.8%) | calendar, contextlib, datetime, decimal, difflib, fractions, functools, hmac, pprint, shlex, statistics, tokenize, tomllib, typing, urllib, warnings, zoneinfo | 77 |
| zlib | −548 | −656 | −540 | 0 | 9 (0.5%) | alone | 61 |
| csv | −5,956 | −6,064 | −5,948 | 0 | 110 (6.2%) | alone | 54 |
| base64 | −3,788 | −3,888 | −3,780 | 29 (1.0%) | 45 (2.5%) | alone | 51 |
| json | −37,847 | −40,960 | −37,962 | 488 (17.2%) | 986 (55.3%) | alone | 26 |
| asyncio | −50,591 | −56,432 | −50,774 | 1,722 (60.6%) | 647 (36.3%) | alone | 21 |

Reading it:

- **25 modules can be removed alone.** Most of the rest take a few small
  importers with them, but four pull in large groups: `operator` (imported at import time by `datetime`, `time`,
  `os`, `hashlib`, `random` and `struct`), `re`, `collections`/`heapq`
  and `functools`. Their group numbers are not their own size: the
  whole stdlib except `struct` and `operator` is −449,725, so those two
  together are about 19.4 KB [M by difference].
- **13 modules are used by neither corpus**: `copy`, `traceback`,
  `dataclasses`, `string`, `logging`, `difflib`, `pprint`, `contextlib`,
  `codecs`, `platform`, `posixpath`, `gc` and `tempfile`. No other
  default module imports them, so the default list without them builds
  and works: 2,038,682 B of hello.wasm (−43,569, −2.1%), native −44,784,
  guest −43,466, with no change to any real cell or synthetic solution
  [M].
- **The heavy hitters are cheap per use.** `asyncio` (−50,591, an
  engine feature since 61f216c) and `json` (−37,847) cost 21 and 26 B per use. `re`
  is the largest group, but 39% of synthetic solutions use it.
- **Most of the bytes per use are in the long tail.** `types`,
  `tokenize`, `shlex`, `tomllib`, `typing`, `struct` and `enum` cost
  650–2,200 B per use, each used by under 2% of either corpus.
- Against size-map.md (a72abfe, before size pass 2 and M8): `decimal`
  is −28,919 now (−39,863 then), `statistics` −10,430 (−13,863),
  `zoneinfo` −8,369 (−10,126). Size pass 2's sectioned bytecode shrank
  them. `asyncio` and `json` gate engine code now: −50,591 and −37,847
  (0 then).

## Profiles

Sizes and failures for each measured set [M]. "Fails" counts the units
whose result moves from the default build's committed one: real cells
from completed to failed, synthetic solutions from pass to error. Static
counts (`census.py`: units that import a module outside the set) are in
parentheses. Some of those units fail before reaching the import, or
already fail on the default build.

| set | modules | hello.wasm | native hello | guest | real fails (static) | synthetic fails (static) |
|---|---:|---:|---:|---:|---:|---:|
| default | 55 | 2,082,251 | 2,265,576 | 2,166,578 | 0 | 0 |
| **core35** (recommended contents) | 35 | 1,996,756 (−85,495, −4.1%) | 2,176,104 (−89,472) | 2,081,318 (−85,260) | **0** (4) | **42** (47) |
| core32: ≥2% of either corpus, plus what passing real cells need | 32 | 1,992,197 (−90,054) | 2,171,240 (−94,336) | 2,076,783 (−89,795) | 0 (7) | 69 (74) |
| default minus the 13 unused modules | 42 | 2,038,682 (−43,569) | 2,220,792 (−44,784) | 2,123,112 (−43,466) | 0 (0) | 0 (0) |
| core36: core35 plus shlex | 36 | 1,999,484 (−82,767) | 2,178,920 (−86,656) | 2,084,038 (−82,540) | 0 (4) | 39 (44) |
| ≥2% plus every module a real cell imports | 34 | 2,001,240 (−81,011) | 2,182,024 (−83,552) | 2,085,782 (−80,796) | 0 (0) | 60 (65) |
| ≥2% of either corpus | 27 | 1,959,591 (−122,660) | 2,135,432 (−130,144) | 2,044,239 (−122,339) | 18 (47) | 69 (74) |
| ≥5% of either corpus | 16 | 1,864,837 (−217,414) | 2,038,472 (−227,104) | 1,949,629 (−216,949) | 48 (77) | 350 (383) |
| every module a real cell imports | 22 | 1,878,572 (−203,679) | 2,053,416 (−212,160) | 1,963,242 (−203,336) | 0 (0) | 540 (584) |
| **real-min**: smallest passing every real cell | 20 | 1,869,455 (−212,796, −10.2%) | 2,042,600 (−222,976) | 1,954,165 (−212,413) | **0** (7) | **549** (593) |
| no stdlib module (size only) | 0 | 1,613,129 (−469,122) | 1,766,936 (−498,640) | 1,696,629 (−469,949) | | |

Which modules the failures need [M, behaviour runs]:

- **core35**: synthetic `zoneinfo` 21, `calendar` 9, `tomllib` 9,
  `shlex` 3. Real: none. Two real cells that already fail change their
  message: `import os, re, io, tokenize` now fails on `tokenize`
  instead of on a later `Path.read_text`.
- **≥2%**: real `typing` 9, `os` 4, `pathlib` 4, `struct` 1. That is why
  core32 adds `typing`, `os`, `pathlib`, `fnmatch` and `struct`
  (32,606 B). core35 adds `textwrap`, `hmac` and `zlib` to core32
  (4,559 B for 27 solutions).
- **real-min**: synthetic `datetime` 182, `csv` 103, `statistics` 64,
  `html` 62, `urllib.parse` 47, `decimal` 45, and the core35 tail.
- **≥5%**: real `base64` 29 (Grok decoding file contents) besides the
  ≥2% ones.

What the remaining failures would cost to fix on top of core35, from the
default-build removals [E]:

| add | bytes | synthetic solutions recovered |
|---|---:|---:|
| `zoneinfo` with `calendar` and `enum` | about 23,000 | 30 |
| `tomllib` with `types` | about 9,400 | 9 |
| `shlex` | 2,728 [M, core36 − core35] | 3 |

**real-min is the floor.** Each of its modules is imported, at import
time, by at least one real cell that completes on the default build
(census), and the import-time closure cannot shrink further. Removing a
module that only one or two passing cells use fails those cells:
each of `random` (with `hashlib` and `bisect`), `io` and `struct`,
removed from real-min in its own build, fails 1 passing real cell, and
`typing` fails 9 [M, `behaviour-minimality.json`]. The other modules of
real-min are each needed by 5 or more passing cells, except `itertools`,
which the one `random` cell needs, and `math`, which gates nothing.

## Bench and import time

Bench programs, whole-process Ir (millions), `mm run` with the default
list and with core35 [M]:

| program | default | core35 |
|---|---:|---:|
| startup | 0.9 | 0.9 |
| fib | 321.5 | 321.5 |
| loop | 1,114.9 | 1,114.9 |
| dict | 1,160.9 | 1,160.9 |
| str | 494.2 | 494.2 |
| json | 1,134.9 | 1,134.9 |
| closure | 714.7 | 714.7 |
| classes | 1,382.0 | 1,382.0 |
| generators | 2,380.9 | 2,380.9 |
| exceptions | 992.3 | 992.3 |
| fmt | 1,562.1 | 1,562.1 |
| sort | 1,453.5 | 1,453.5 |
| interp | 2,247.4 | 2,247.7 |

Import Ir (`imp`, steady state with the decoded-module cache, and the
first operation in a fresh process) [M]:

| import | steady default | steady core35 | first default | first core35 |
|---|---:|---:|---:|---:|
| none (session only) | 496,223 | 495,640 (−0.12%) | 838,143 | 836,507 (−0.20%) |
| json | 1,173,241 | 1,172,266 | 2,140,404 | 2,136,226 |
| re | 2,892,402 | 2,892,268 | 6,486,706 | 6,482,959 |
| collections | 3,028,084 | 3,028,751 | 6,223,127 | 6,223,684 |
| asyncio | 651,588 | 650,701 | 995,752 | 993,822 |
| datetime | 6,330,924 | 6,329,470 | 12,502,133 | 12,489,526 |
| typing | 7,227,060 | 7,242,190 | 13,498,592 | 13,346,850 |
| decimal | 33,419,053 | 33,450,536 | 46,509,847 | 46,498,047 |
| statistics | 56,321,604 | 56,496,168 | 75,906,969 | 75,870,514 |
| json, re, collections, asyncio, datetime | 13,031,378 | 12,948,336 | 24,811,487 | 24,799,580 |

The differences are within ±0.7%, in both directions, with no trend.
Wall-clock medians in the orb (`import-wall-*.json`, 51 runs each) move
by up to 80 µs either way, which is within their run-to-run noise.
Removing modules saves bytes and nothing else.

## Recommendation

**Don't ship `stdlib-codemode-core` now.** Fix the eight undeclared
imports in `crates/mm-stdlib/Cargo.toml` instead, and document core35
in docs/sdk.md as the measured list for size-sensitive embedders.

- **The saving is small.** −85,495 B is 4.1% of hello.wasm. Most of it
  is modules nobody used (43,569 B) and the long tail. For
  comparison, size-map.md's structural levers are of the same order or
  larger and apply to every build: solid or chunked bytecode streams
  (−29 to −107 KB), and `#[optimize(size)]` on cold code once Rust
  1.101 has it (−112 KB).
- **It has a measured cost.** 42 synthetic solutions (2.4%) that pass
  today fail with an import error. It has no speed or memory benefit.
- **A named feature is a second configuration to maintain.** It has its
  own build id, so snapshots don't cross between it and the default
  build. It needs its own `cargo xtask size` line, durability and
  wasm-host runs, and its list must change whenever a module moves.
  An embedder who wants the bytes can already write the list.
- **A hand-written list is unsafe until the graph is fixed.** With
  today's manifests, `features = ["mod-typing"]` builds a `typing`
  that cannot be imported. After the fix, core35 needs no more than
  its 35 names.

If the owner ships it anyway, add it to `crates/micro-monty/Cargo.toml`
as follows, listing every module so that it does not depend on the fix:

```toml
# The codemode profile (docs/research/stdlib-profile.md): every module at
# least 2% of either codemode corpus uses, every module a passing real plak
# cell needs, and textwrap, hmac and zlib (4.6 KB, 27 solutions). Use with
# `default-features = false`.
stdlib-codemode-core = [
    "mod-json", "mod-math", "mod-re", "mod-collections", "mod-itertools",
    "mod-functools", "mod-datetime", "mod-random", "mod-asyncio", "mod-typing",
    "mod-base64", "mod-hashlib", "mod-operator", "mod-heapq", "mod-bisect",
    "mod-textwrap", "mod-abc", "mod-binascii", "mod-time", "mod-os",
    "mod-pathlib", "mod-fnmatch", "mod-csv", "mod-io", "mod-urllib",
    "mod-struct", "mod-html", "mod-warnings", "mod-decimal", "mod-numbers",
    "mod-contextvars", "mod-fractions", "mod-statistics", "mod-hmac", "mod-zlib",
]
```

It leaves out 20 default modules: `calendar`, `codecs`, `contextlib`,
`copy`, `dataclasses`, `difflib`, `enum`, `gc`, `logging`, `platform`,
`posixpath`, `pprint`, `shlex`, `string`, `tempfile`, `tokenize`,
`tomllib`, `traceback`, `types` and `zoneinfo`. Its sizes are 1,996,756 /
2,176,104 / 2,081,318 [M]. It also needs a `cargo xtask size` line and a
build in `crates/micro-monty/tests/features.rs`.

## Caveats

- Usage is static: an import counts if the source has it. The behaviour
  runs give the counts that matter, and they are lower (core35: 4 real
  cells and 47 solutions import something outside it; 0 and 42 fail).
- Both corpora are what we have. The real one is 8 code-review prompts
  across two model families; the synthetic one is 198 API and data tasks.
  Logging, dataclasses and contextlib, unused in both, are common in
  longer-lived agent code. A host whose agents write such code should
  keep the default.
- Uses of a module through an import-time dependency count for that
  module, so `operator`, `heapq` and `collections` have more uses than
  direct importers (synthetic `operator`: 358 uses, 0 direct).
- Sizes are for `wasm32-unknown-unknown` and x86-64 Linux only. wasip1
  and compressed sizes were not measured.
