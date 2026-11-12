# Python engines: medians

## Capability

Monty corpus, `cpython`-tagged cases (judged by conformance/oracle.py):

| Engine | Cases | Pass | Fail | Crash | Top causes |
|---|---:|---:|---:|---:|---|
| mm | 568 | 546 | 22 | 0 | semantics 17, missing builtin 5 |
| micropython | 568 | 78 | 488 | 2 | traceback format 140, semantics 126, exception message 87, missing attribute or method 41, syntax 38, missing builtin 9, missing module: typing 9, missing module: dataclasses 8 |
| pocketpy | 568 | 53 | 502 | 13 | syntax 148, semantics 113, traceback format 97, exception message 72, missing attribute or method 44, crash 13, missing builtin 7, missing module: itertools 6 |

conformance/own as scripts vs CPython (stdout and exit status; strict adds stderr):

| Engine | Cases | Pass | Strict | Top causes |
|---|---:|---:|---:|---|
| mm | 278 | 278 | 278 |  |
| micropython | 278 | 10 | 7 | semantics 106, missing attribute or method 53, syntax 35, missing builtin 16, missing module: enum 8, not implemented 5, timeout 5, missing module: unicodedata 4 |
| pocketpy | 278 | 3 | 3 | syntax 153, semantics 64, missing attribute or method 30, missing builtin 3, crash 3, missing module: string 2, missing module: calendar 2, missing module: __future__ 1 |

Codemode solutions that pass on CPython (1722 of 1793):

| Engine | Pass | Top causes |
|---|---:|---|
| mm | 1701 | missing module: xml 18, missing module: email 3 |
| micropython | 829 | missing attribute or method 179, missing module: collections.Counter 177, semantics 147, missing module: csv 101, missing module: statistics 63, missing module: decimal 44, missing module: urllib 43, missing module: html.parser 40 |
| pocketpy | 0 | syntax 1682, crash 40 |
| pocketpy-sync | 283 | syntax 908, missing module: re 100, semantics 80, missing attribute or method 73, missing module: csv 69, crash 38, missing module: urllib.parse 26, missing module: hashlib 24 |

## Size

| Engine | Target | Bytes | gzip -9 | Build |
|---|---|---:|---:|---|
| mm | native | 2,409,464 | 1,409,980 | `cargo xtask size` hello |
| micropython | native | 547,688 | 238,373 | sandbox variant, upstream -Os, stripped |
| micropython-standard | native | 839,040 | 393,906 | unix standard variant, -Os, stripped |
| pocketpy | native | 462,088 | 232,304 | pocketpy.c + hello, -Os -flto, gc-sections, stripped |
| pocketpy -O2 host build | native | 695,824 | 315,178 | upstream CLI, -O2 (speed build), not stripped |
| mm | wasm32-unknown-unknown | 2,203,338 | 1,083,582 | `cargo xtask size` hello.wasm |
| mm-wasm guest | wasm32-unknown-unknown | 2,289,389 | 1,117,377 | `cargo xtask size` |
| micropython | wasm32 (emscripten) | 449,553 | 197,274 | npm @micropython/micropython-webassembly-pyscript 1.29.0-6 (+ 110 KB .mjs glue) |
| pocketpy | wasm32-wasip1 | 451,815 | 192,262 | wasi-sdk 34 -Oz -flto, wasm-opt -Oz |

## Start-up (µs)

| Engine | New runtime | New session | Codemode setup | First cell |
|---|---:|---:|---:|---:|
| mm | 22.4 | 51.5 | 62.8 | 31.3 |
| micropython | 1,389.5 | 0.4 | 128.4 | 6.5 |
| pocketpy | 1,284.3 | 540.1 | 38.3 | 4.4 |

## Memory (KB added by N live sessions: RSS / malloc heap)

| Engine | empty 1 | empty 8 | empty 40 | setup 1 | setup 8 | setup 40 |
|---|---:|---:|---:|---:|---:|---:|
| mm | 0 / 111 | 1,401 / 901 | 3,564 / 4,589 | 0 / 127 | 602 / 1,012 | 5,382 / 5,084 |
| micropython | 0 / -0 | 0 / 2 | 0 / 3 | 0 / 2 | 0 / 2 | 0 / 3 |
| pocketpy | 524 / 17 | 5,657 / 8,059 | n/a | 528 / 13 | 5,669 / 8,122 | n/a |

micropython GC heap in use after N set-up sessions: 1: 7,552 B (base 640), 8: 19,520 B (base 640), 40: 74,432 B (base 640)

## Host calls (µs per call: tool minus local, and total)

| Engine | Sync | Sync total | Awaited | Awaited total | Fan-out | Fan-out total |
|---|---:|---:|---:|---:|---:|---:|
| mm | 0.36 | 0.41 | 2.95 | 3.32 | 2.73 | 3.38 |
| micropython | 0.13 | 0.21 | 2.02 | 2.20 | 2.02 | 5.30 |
| pocketpy | 0.02 | 0.12 | — | — | — | — |

## Codemode workload (ms per task: new session + setup + run; run alone in parentheses)

| Task | mm | micropython | pocketpy |
|---|---:|---:|---:|
| agg-avg-order-by-tier | 1.41 (1.26) | 2.04 (1.84) | fails |
| agg-customer-summary | 0.41 (0.26) | 0.42 (0.26) | 0.88 (0.20) |
| agg-revenue-by-region | 0.23 (0.11) | 0.26 (0.10) | 0.74 (0.08) |
| agg-top-customers | 0.24 (0.12) | 0.28 (0.11) | 0.71 (0.08) |
| logs-access-top-ips | 2.56 (2.40) | fails | fails |
| logs-errors-per-service | 1.87 (1.72) | 3.02 (2.79) | fails |
| neg-fanout-40 | 0.68 (0.56) | 1.08 (0.89) | fails |
| page-accounts-offset | 0.61 (0.47) | 0.68 (0.51) | fails |
| page-all-pending | 0.44 (0.31) | 0.49 (0.32) | fails |
| page-feb-orders | 0.76 (0.64) | fails | fails |
| page-open-bugs | 0.29 (0.18) | 0.37 (0.21) | 0.82 (0.19) |
| par-two-level-fanout | 0.91 (0.77) | 0.72 (0.53) | fails |
| par-weather-warmest | 0.39 (0.27) | 0.38 (0.23) | fails |
| rep-quotes-csv | 0.39 (0.25) | 0.56 (0.34) | fails |
| reshape-nested-counts | 0.98 (0.84) | fails | fails |
| retry-backoff | 0.31 (0.19) | 0.31 (0.16) | fails |
| retry-gather-flaky | 0.55 (0.42) | 0.56 (0.40) | fails |
| retry-give-up | 0.49 (0.37) | 0.51 (0.36) | fails |
| retry-permanent-errors | 0.43 (0.30) | 0.43 (0.28) | fails |
| stat-max-drawdown | 0.72 (0.56) | 1.14 (0.95) | fails |
| wf-close-fixed | 0.87 (0.70) | 0.99 (0.81) | fails |
| wf-customer-latest-order | 0.60 (0.43) | 0.72 (0.53) | 1.17 (0.46) |
| **sum, total** | 16.1 | 15.0 | 4.3 |
| **sum, run** | 13.1 | 11.6 | 1.0 |
| **tasks that ran** | 22 | 19 | 5 |

## Bench programs in-process (ms per run)

| Program | mm | micropython | pocketpy |
|---|---:|---:|---:|
| startup | 0.1 | 0.0 | 0.6 |
| fib | 23.4 | 38.1 | 67.3 |
| loop | 74.0 | 131.7 | 151.8 |
| dict | 129.3 | fails | 99.6 |
| str | 49.3 | 139.3 | 36.4 |
| json_rows | 113.4 | 9,338.4 | fails |
| closure | 46.6 | 81.9 | 95.1 |
| classes | 105.6 | 125.2 | 184.3 |
| generators | 196.0 | 67.3 | fails |
| exceptions | 101.7 | 155.7 | fails |
| fmt | 196.0 | 250.5 | fails |
| sort | 143.3 | 5,134.5 | fails |
| interp | 212.0 | fails | fails |

## Bench programs as CLI processes (ms, spawn to exit)

| Program | mm | micropython | pocketpy |
|---|---:|---:|---:|
| startup | 4.8 | 6.4 | 6.7 |
| fib | 29.2 | 46.9 | 85.0 |
| loop | 70.7 | 140.2 | 198.4 |
| dict | 123.7 | fails | 126.0 |
| str | 53.0 | 160.3 | 47.4 |
| json_rows | 116.8 | 9,494.2 | fails |
| closure | 52.0 | 91.7 | 117.1 |
| classes | 107.1 | 133.0 | 246.3 |
| generators | 193.9 | 76.0 | fails |
| exceptions | 102.6 | 162.2 | fails |
| fmt | 194.3 | 283.9 | fails |
| sort | 142.7 | 5,317.9 | fails |
| interp | 221.1 | fails | fails |

## Polling overhead (stop armed vs not)

| Engine | loop | fib | sort |
|---|---:|---:|---:|
| mm | +1.1% | -0.9% | -0.3% |
| micropython | -3.0% | +1.0% | -2.2% |
| pocketpy | +3479.4% | +2840.4% | — |

## Control (ms to regain control; outcome)

| Engine | loop | sort | string | sort-loop | string-loop | bigint | catch | catch-all |
|---|---|---|---|---|---|---|---|---|
| mm | 0.06 ms, parked, resumable True, usable True, n=5 | 1.49 ms, parked, resumable True, usable True, n=5 | 0.31 ms, parked, resumable True, usable True, n=5 | 1.64 ms, parked, resumable True, usable True, n=5 | 1.00 ms, parked, resumable True, usable True, n=5 | — ms, error, resumable False, usable True, n=5 | 0.09 ms, parked, resumable True, usable True, n=5 | 0.08 ms, parked, resumable True, usable True, n=5 |
| micropython | 0.08 ms, interrupted, resumable False, usable True, n=5 | 703.94 ms, interrupted, resumable False, usable True, n=5 | 156.21 ms, interrupted, resumable False, usable True, n=5 | 764.57 ms, interrupted, resumable False, usable True, n=5 | 67.55 ms, interrupted, resumable False, usable True, n=5 | 3,274.98 ms, interrupted, resumable False, usable True, n=5 | 0.09 ms, interrupted, resumable False, usable True, n=5 | never (10 s), n=5 |
| pocketpy | 0.10 ms, interrupted, resumable False, usable True, n=5 | 434.35 ms, interrupted, resumable False, usable True, n=5 | 74.83 ms, interrupted, resumable False, usable True, n=5 | 446.23 ms, interrupted, resumable False, usable True, n=5 | 82.22 ms, interrupted, resumable False, usable True, n=5 | 0.11 ms, interrupted, resumable False, usable True, n=5 | never (10 s), n=5 | never (10 s), n=5 |

## GC pauses (ms; 20,000,000 one-item lists of garbage beside N live ones, `hsync` every 1,000)

Steady: longest gap / p99 gap / median gap while making garbage, and how many gaps passed 10 ms. Build: longest gap while the live set grew. Drop: `live = None`. Total: the whole script.

| Engine | Live | Garbage | Steady max | p99 | median | >10 ms | Build max | Drop | Total | Peak RSS MB |
|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| mm | 0 | acyclic | 0.23 | 0.09 | 0.047 | 0 | — | 0.00 | 1,009 | 8 |
| mm | 0 | cyclic | 0.65 | 0.35 | 0.170 | 0 | — | 0.00 | 3,656 | 8 |
| mm | 1,000,000 | acyclic | 5.80 | 0.16 | 0.048 | 0 | 16.66 | 0.00 | 1,518 | 147 |
| mm | 1,000,000 | cyclic | 11.96 | 0.32 | 0.173 | 27 | 16.51 | 0.00 | 4,385 | 148 |
| mm | 2,000,000 | acyclic | 5.66 | 0.18 | 0.048 | 0 | 16.34 | 0.00 | 2,069 | 175 |
| mm | 2,000,000 | cyclic | 12.00 | 0.32 | 0.172 | 66 | 16.46 | 0.00 | 5,297 | 175 |
| micropython | 0 | acyclic | 19.96 | 0.11 | 0.066 | 4 | — | 0.00 | 1,387 | 259 |
| micropython | 0 | cyclic | 18.93 | 0.14 | 0.095 | 4 | — | 0.00 | 2,036 | 259 |
| micropython | 1,000,000 | acyclic | 50.83 | 0.12 | 0.065 | 6 | 5.47 | 0.00 | 1,751 | 259 |
| micropython | 1,000,000 | cyclic | 50.79 | 0.14 | 0.094 | 6 | 5.43 | 0.00 | 2,377 | 259 |
| micropython | 2,000,000 | acyclic | 74.64 | 0.10 | 0.063 | 10 | 11.00 | 0.00 | 2,296 | 259 |
| micropython | 2,000,000 | cyclic | 81.38 | 0.18 | 0.094 | 10 | 10.94 | 0.00 | 2,983 | 259 |
| pocketpy | 0 | acyclic | 7.01 | 0.17 | 0.070 | 0 | — | 0.00 | 2,133 | 27 |
| pocketpy | 0 | cyclic | 7.14 | 0.24 | 0.118 | 0 | — | 0.00 | 3,162 | 26 |
| pocketpy | 1,000,000 | acyclic | 38.80 | 0.17 | 0.074 | 126 | 24.74 | 0.00 | 6,832 | 199 |
| pocketpy | 1,000,000 | cyclic | 36.55 | 0.29 | 0.123 | 126 | 24.39 | 0.00 | 7,835 | 199 |
| pocketpy | 2,000,000 | acyclic | 66.33 | 0.16 | 0.074 | 126 | 49.15 | 0.00 | 13,689 | 371 |
| pocketpy | 2,000,000 | cyclic | 70.52 | 0.21 | 0.120 | 126 | 48.35 | 0.00 | 14,548 | 370 |
| monty | 0 | acyclic | 0.23 | 0.13 | 0.071 | 0 | — | 0.00 | 1,492 | 7 |
| monty | 0 | cyclic | 12.37 | 1.04 | 0.169 | 4 | — | 0.01 | 5,102 | 27 |
| monty | 1,000,000 | acyclic | 0.23 | 0.13 | 0.076 | 0 | 65.65 | 50.13 | 2,250 | 167 |
| monty | 1,000,000 | cyclic | 23.67 | 0.53 | 0.168 | 200 | 66.12 | 49.00 | 7,824 | 179 |
| monty | 2,000,000 | acyclic | 0.25 | 0.10 | 0.073 | 0 | 130.49 | 95.55 | 3,534 | 327 |
| monty | 2,000,000 | cyclic | 38.14 | 0.54 | 0.165 | 200 | 132.23 | 109.57 | 11,212 | 334 |

## Snapshots mid-await (mm only)

| Point | Raw bytes | zstd-1 bytes | Snapshot µs | Restore µs | Restored run finished |
|---|---:|---:|---:|---:|---|
| neg-fanout-40@0 | 69,535 | 16,249 | 65.2 | 89.7 | True |
| agg-avg-order-by-tier@3 | 144,586 | 26,655 | 118.9 | 173.0 | True |
| par-two-level-fanout@1 | 170,197 | 39,699 | 166.2 | 259.7 | True |

Workload tasks the checker failed (not timed above):

- micropython `logs-access-top-ips`: FAIL Traceback (most recent call last):
- micropython `page-feb-orders`: FAIL Traceback (most recent call last):
- micropython `reshape-nested-counts`: FAIL Traceback (most recent call last):
- pocketpy `agg-avg-order-by-tier`: FAIL Traceback (most recent call last):
- pocketpy `logs-access-top-ips`: FAIL Traceback (most recent call last):
- pocketpy `logs-errors-per-service`: FAIL Traceback (most recent call last):
- pocketpy `neg-fanout-40`: FAIL Traceback (most recent call last):
- pocketpy `page-accounts-offset`: FAIL Traceback (most recent call last):
- pocketpy `page-all-pending`: FAIL Traceback (most recent call last):
- pocketpy `page-feb-orders`: FAIL Traceback (most recent call last):
- pocketpy `par-two-level-fanout`: FAIL Traceback (most recent call last):
- pocketpy `par-weather-warmest`: FAIL Traceback (most recent call last):
- pocketpy `rep-quotes-csv`: FAIL Traceback (most recent call last):
- pocketpy `reshape-nested-counts`: FAIL Traceback (most recent call last):
- pocketpy `retry-backoff`: FAIL Traceback (most recent call last):
- pocketpy `retry-gather-flaky`: FAIL Traceback (most recent call last):
- pocketpy `retry-give-up`: FAIL Traceback (most recent call last):
- pocketpy `retry-permanent-errors`: FAIL Traceback (most recent call last):
- pocketpy `stat-max-drawdown`: FAIL Traceback (most recent call last):
- pocketpy `wf-close-fixed`: FAIL Traceback (most recent call last):

## Hostile inputs (outcome; ms; same session usable after)

| Case | mm | micropython | pocketpy |
|---|---|---|---|
| alloc-format-1GB | MemoryError; 0 ms; usable | timeout (60 s) | ok: 1000000000; 5,182 ms; usable |
| alloc-grow | MemoryError; 137 ms; usable | MemoryError: memory allocation failed, allocating 1000001 bytes; 1,537 ms; usable | host died: signal 11 |
| alloc-grow-caught | ok: caught True; 138 ms; usable | ok: caught True; 1,539 ms; usable | host died: signal 11 |
| alloc-list-1TiB | MemoryError; 0 ms; usable | MemoryError: memory allocation failed, allocating 8796093022208 bytes; 8 ms; usable | ok: ; 0 ms; usable |
| alloc-str-1TiB | MemoryError; 0 ms; usable | MemoryError: memory allocation failed, allocating 1099511627777 bytes; 7 ms; usable | host died: signal 11 |
| bigint-1e8-digits | MemoryError; 0 ms; usable | timeout (60 s) | ok: 0; 0 ms; usable |
| fuzz-asan | MemoryError: integer operation too large; 0 ms; usable | timeout (60 s) | SyntaxError: int literal is too large; 0 ms; usable |
| fuzz-excrender | KeyError: frozenset({(frozenset({(frozenset({(frozenset({frozenset({frozenset({(frozenset({(frozenset({(frozenset({frozenset({frozenset({frozenset({(frozenset({; 34 ms; usable | timeout (60 s) | NameError: name 'frozenset' is not defined; 0 ms; usable |
| fuzz-strindex | ok: ; 3 ms; usable | ok: ; 307 ms; usable | SyntaxError: invalid escape char; 0 ms; usable |
| nest-data-eq | RecursionError: maximum recursion depth exceeded in comparison; 181 ms; usable | host died: signal 6 | host died: signal 11 |
| nest-data-json | RecursionError: maximum recursion depth exceeded while encoding a JSON object; 94 ms; usable | RuntimeError: maximum recursion depth exceeded; 83 ms; usable | host died: signal 6 |
| nest-data-repr | RecursionError: maximum recursion depth exceeded while getting the repr of an object; 88 ms; usable | RuntimeError: maximum recursion depth exceeded; 80 ms; usable | host died: signal 6 |
| nest-list-source | SyntaxError: too many nested parentheses; 1 ms; usable | ok: ; 40,401 ms; usable | host died: signal 6 |
| nest-parens-source | SyntaxError: too many nested parentheses; 1 ms; usable | ok: ; 37,617 ms; usable | host died: signal 6 |
| recursion | RecursionError: maximum recursion depth exceeded; 0 ms; usable | RuntimeError: maximum recursion depth exceeded; 8 ms; usable | RecursionError: maximum recursion depth exceeded; 0 ms; usable |
| recursion-caught | ok: caught; 0 ms; usable | NameError: name 'RecursionError' isn't defined; 6 ms; usable | ok: caught; 0 ms; usable |
| recursion-generator | RecursionError: maximum recursion depth exceeded; 1 ms; usable | RuntimeError: maximum recursion depth exceeded; 25 ms; usable | RecursionError: maximum recursion depth exceeded; 1 ms; usable |
| recursion-repr | RecursionError: maximum recursion depth exceeded; 1 ms; usable | RuntimeError: maximum recursion depth exceeded; 5 ms; usable | RecursionError: maximum recursion depth exceeded; 0 ms; usable |
| regex-redos | ok: None; 21,426 ms; usable | ok: None; 5,471 ms; usable | SyntaxError: expected '@id', got 'match'; 0 ms; usable |

## Isolation (session B after session A patched builtins.len, json.dumps, asyncio.gather)

| Engine | Session B sees | Session A sees | Isolated |
|---|---|---|---|
| mm | `[2, '1', True]` | `[2, 'patched', False]` | True |
| micropython | `[-1, '1', False]` | `[-1, '1', False]` | False |
| pocketpy | `[2, '1', True]` | `[-1, 'patched', True]` | True |

## Errors (host processes that failed, each listed once)

- {"engine": "pocketpy", "section": "mem", "args": ["mem", "empty", "40", "/home/user/workspace/repo/bench/evidence/engines/fixtures/neg-fanout-40.json"], "error": "error: PocketPy allows at most 16 VMs per process\n", "file": null}
- {"engine": "pocketpy", "section": "mem", "args": ["mem", "setup", "40", "/home/user/workspace/repo/bench/evidence/engines/fixtures/neg-fanout-40.json"], "error": "error: PocketPy allows at most 16 VMs per process\n", "file": null}
- {"engine": "pocketpy", "section": "hostcall", "args": ["hostcall", "await", "10000", "5"], "error": "error: Traceback (most recent call last):\n  File \"cell.py\", line 2\n    await noop({'i': i})\nSyntaxError: expected statement end\n", "file": null}
- {"engine": "pocketpy", "section": "hostcall", "args": ["hostcall", "fanout", "100", "5"], "error": "error: Traceback (most recent call last):\n  File \"cell.py\", line 3\n    await asyncio.gather(*[noop({'i': i}) for i in range(100)])\nSyntaxError: expected statement end\n", "file": null}
- {"engine": "micropython", "section": "bench", "args": null, "error": "skipped", "file": "dict.py"}
- {"engine": "pocketpy", "section": "bench", "args": ["bench", "/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/json_rows.py", "3"], "error": "error: Traceback (most recent call last):\n  File \"cell.py\", line 7\n    text = json.dumps(rows, separators=(',', 
- {"engine": "pocketpy", "section": "bench", "args": ["bench", "/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/generators.py", "3"], "error": "error: Traceback (most recent call last):\n  File \"cell.py\", line 30\n    squares = sum(x * x % 1000 for x in ran
- {"engine": "pocketpy", "section": "bench", "args": ["bench", "/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/exceptions.py", "3"], "error": "error: Traceback (most recent call last):\n  File \"cell.py\", line 37\n    else:\nSyntaxError: expected an express
- {"engine": "pocketpy", "section": "bench", "args": ["bench", "/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/fmt.py", "3"], "error": "error: Traceback (most recent call last):\n  File \"cell.py\", line 6\n    head = f'{name}|{i % 97:>4}|{i * 31 % 4096:x}'\
- {"engine": "pocketpy", "section": "bench", "args": ["bench", "/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/sort.py", "3"], "error": "error: Traceback (most recent call last):\n  File \"cell.py\", line 11\n    flat = sorted([v for row in grid for v in row
- {"engine": "pocketpy", "section": "bench", "args": ["bench", "/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/interp.py", "3"], "error": "error: Traceback (most recent call last):\n  File \"cell.py\", line 126\n    n, s = run(source.replace('10007', str(100
- {"engine": "micropython", "section": "bench", "args": ["bench", "/home/user/workspace/repo/bench/interp.py", "3"], "error": "error: Traceback (most recent call last):\n  File \"cell.py\", line 126, in <module>\n  File \"cell.py\", line 110, in run\n  File \"cell.py\", line 27, in tokenize\nAttribute
- {"engine": "micropython", "section": "cli", "args": null, "error": "skipped", "file": "dict.py"}
- {"engine": "pocketpy", "section": "cli", "args": null, "error": "Traceback (most recent call last):\n  File \"/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/json_rows.py\", line 7\n    text = json.dumps(rows, separators=(',', ':'))\nTypeError: 'separators'
- {"engine": "pocketpy", "section": "cli", "args": null, "error": "Traceback (most recent call last):\n  File \"/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/generators.py\", line 30\n    squares = sum(x * x % 1000 for x in range(300000))\nSyntaxError: expe
- {"engine": "pocketpy", "section": "cli", "args": null, "error": "Traceback (most recent call last):\n  File \"/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/exceptions.py\", line 37\n    else:\nSyntaxError: expected an expression, got else\n", "file": "exc
- {"engine": "pocketpy", "section": "cli", "args": null, "error": "Traceback (most recent call last):\n  File \"/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/fmt.py\", line 6\n    head = f'{name}|{i % 97:>4}|{i * 31 % 4096:x}'\nValueError: invalid format sp
- {"engine": "pocketpy", "section": "cli", "args": null, "error": "Traceback (most recent call last):\n  File \"/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/sort.py\", line 11\n    flat = sorted([v for row in grid for v in row if v])\nSyntaxError: expected
- {"engine": "micropython", "section": "cli", "args": null, "error": "ck (most recent call last):\n  File \"/home/user/workspace/repo/bench/interp.py\", line 126, in <module>\n  File \"/home/user/workspace/repo/bench/interp.py\", line 110, in run\n  File \"/home/user/workspace/repo/bench/interp.py\", 
- {"engine": "pocketpy", "section": "cli", "args": null, "error": "s/pocketpy/interp.py\", line 110, in run\n    statements = Parser(tokenize(source)).program()\n  File \"/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/interp.py\", line 17, in tokenize\n    i
- {"engine": "pocketpy", "section": "polling", "args": ["polling", "/home/user/workspace/repo/bench/evidence/python-engines/results/2026-11-09/inputs/pocketpy/sort.py", "3"], "error": "error: Traceback (most recent call last):\n  File \"cell.py\", line 11\n    flat = sorted([v for row in grid for v in
