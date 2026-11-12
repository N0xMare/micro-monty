# Engine comparison: medians

## Headline

| Figure | mm-native | mm-wasm | rquickjs | dquickts | quickjs-wasi | djs | djs-wasm |
|---|---:|---:|---:|---:|---:|---:|---:|
| Minimal one-script host, bytes | 2,409,456 | 2,203,338 | 1,281,232 | — | 637,405 | 1,600,432 | 929,693 |
| RSS per set-up session (40 live), KB | 132 | 459 | 436 | 461 | 620 | 764 | 1,377 |
| RSS per empty session (40 live), KB | 91 | 305 | 183 | 180 | 319 | 216 | 312 |
| New session + codemode setup, ms | 0.10 | 0.58 | 4.10 | 3.31 | 9.70 | 5.15 | 11.28 |
| Session from a saved image, ms | 0.146 | 0.480 | — | 0.350 | 0.673 | 0.036 | — |
| Awaited host call, µs per call | 2.16 | 4.79 | 2.96 | 2.02 | 25.10 | 4.60 | — |
| Fan-out of 100, µs per call | 1.89 | 3.64 | 3.05 | 2.07 | 25.67 | 4.90 | — |
| Sync host call, µs per call | 0.32 | 0.80 | 0.05 | 0.04 | 1.62 | 0.13 | — |
| 22 tasks end to end (sum), ms | 12.5 | 38.4 | 89.4 | 73.6 | 210.3 | 116.6 | — |
| …setup only (sum), ms | 2.3 | 13.8 | 81.9 | 67.1 | 191.1 | 106.0 | — |
| …script run only (sum), ms | 10.2 | 24.7 | 7.2 | 6.4 | 18.9 | 10.5 | — |
| Task end to end, median, ms | 0.44 | 1.39 | 3.95 | 3.25 | 9.33 | 5.27 | — |
| Script run vs mm (geomean) | 1.00 | 2.27 | 0.79 | 0.71 | 2.05 | 1.15 | — |
| 12 bench programs, time vs mm (geomean) | 1.00 | 1.99 | 1.66 | 0.85 | 3.35 | 1.78 | 3.58 |

CLI bench geomean vs `mm run`: qjs-ng 1.08, qjs-bellard 0.94, djs-cli 1.80, mm-cli 1.00.

## Size (bytes; gzip -9)

| Engine | Target | Bytes | gzip -9 | Build |
|---|---|---:|---:|---|
| mm | native | 2,409,456 | 1,409,971 | micro-monty `hello` (cargo xtask size) |
| rquickjs | native | 1,281,232 | 607,705 | size/rquickjs (build.sh) |
| djs | native | 1,600,432 | 775,381 | size/djs (build.sh) |
| qjs-ng | native | 1,334,808 | 644,081 | quickjs-ng qjs CLI (cmake Release, IPO, stripped) |
| qjs-bellard | native | 1,137,952 | 530,717 | QuickJS qjs CLI (make CONFIG_LTO=y, stripped) |
| djs-cli | native | 5,319,648 | 2,208,145 | djs CLI (upstream release profile, stripped) |
| mm | wasm32-unknown-unknown | 2,203,338 | 1,083,582 | micro-monty `hello.wasm` (cargo xtask size) |
| mm-wasm guest | wasm32-unknown-unknown | 2,289,389 | 1,117,377 | mm-wasm v1 guest (cargo xtask size) |
| quickjs-wasi | wasm32-wasip1 | 637,405 | 286,642 | quickjs-wasi 3.6.2 npm module |
| djs-wasm | wasm32-unknown-unknown | 929,693 | 421,721 | djs scripts/build-wasm.sh, then wasm-opt -Oz |

## Start-up (µs)

| Engine | New runtime | New session | Codemode setup | First cell | From image | First cell after image |
|---|---:|---:|---:|---:|---:|---:|
| mm-native | 22.2 | 44.1 | 52.7 | 23.3 | 146.3 | 36.9 |
| mm-wasm | 2710.9 | 143.0 | 436.2 | 50.2 | 480.3 | 74.7 |
| rquickjs | 0.1 | 116.9 | 3982.3 | 26.3 | — | — |
| dquickts | 0.1 | 106.4 | 3203.2 | 23.0 | 349.6 | 19.9 |
| quickjs-wasi | 3127.8 | 360.6 | 9339.9 | 77.5 | 673.0 | 90.6 |
| djs | 0.0 | 15.1 | 5130.3 | 41.7 | 36.0 | 24.9 |
| djs-wasm | 3791.4 | 140.2 | 11140.5 | 22.6 | — | — |

## Memory (KB added by N live sessions: RSS / malloc heap / engine count)

| Engine | empty 1 | empty 8 | empty 40 | setup 1 | setup 8 | setup 40 |
|---|---:|---:|---:|---:|---:|---:|
| mm-native | 0 / 109 / 0 | 1,339 / 897 / 0 | 3,625 / 4,585 / 0 | 0 / 122 / 0 | 2,286 / 1,007 / 0 | 5,288 / 5,085 / 0 |
| mm-wasm | 0 / 32 / 1,507 | 1,946 / 273 / 12,059 | 12,218 / 1,442 / 60,293 | 0 / 33 / 1,507 | 3,367 / 278 / 12,059 | 18,342 / 1,454 / 60,293 |
| rquickjs | 0 / 161 / 107 | 1,819 / 1,339 / 859 | 7,311 / 6,736 / 4,294 | 139 / 438 / 363 | 3,113 / 3,558 / 2,903 | 17,457 / 17,783 / 14,515 |
| dquickts | 0 / 163 / 165 | 1,880 / 1,323 / 1,322 | 7,188 / 6,647 / 6,607 | 0 / 467 / 467 | 3,359 / 3,760 / 3,740 | 18,440 / 18,769 / 18,698 |
| quickjs-wasi | 0 / 42 / 1,376 | 2,437 / 353 / 11,010 | 12,743 / 1,792 / 55,050 | 369 / 45 / 1,638 | 4,899 / 357 / 13,107 | 24,797 / 1,797 / 65,536 |
| djs | 0 / 263 / 200 | 1,618 / 2,100 / 1,597 | 8,647 / 10,508 / 7,985 | 807 / 805 / 738 | 6,214 / 6,380 / 5,904 | 30,544 / 31,635 / 29,522 |
| djs-wasm | 0 / 10 / 590 | 2,273 / 94 / 4,719 | 12,464 / 506 / 23,593 | 1,159 / 11 / 1,573 | 10,953 / 95 / 12,583 | 55,067 / 507 / 62,915 |

## Host calls (µs per call: tool minus local, and total)

| Engine | Sync | Sync total | Awaited | Awaited total | Fan-out of 100 | Fan-out total |
|---|---:|---:|---:|---:|---:|---:|
| mm-native | 0.32 | 0.36 | 2.16 | 2.43 | 1.89 | 2.32 |
| mm-wasm | 0.80 | 0.87 | 4.79 | 5.23 | 3.64 | 4.42 |
| rquickjs | 0.05 | 0.12 | 2.96 | 3.90 | 3.05 | 4.35 |
| dquickts | 0.04 | 0.06 | 2.02 | 2.76 | 2.07 | 3.06 |
| quickjs-wasi | 1.62 | 1.74 | 25.10 | 27.06 | 25.67 | 29.03 |
| djs | 0.13 | 0.17 | 4.60 | 5.31 | 4.90 | 7.06 |

## Codemode workload (ms per task: new session + setup + run)

| Task | mm-native | mm-wasm | rquickjs | dquickts | quickjs-wasi | djs |
|---|---:|---:|---:|---:|---:|---:|
| agg-avg-order-by-tier | 1.14 (1.01) | 2.67 (1.96) | 4.47 (0.77) | 3.80 (0.67) | 10.62 (2.03) | 5.51 (0.94) |
| agg-customer-summary | 0.31 (0.20) | 1.03 (0.39) | 3.75 (0.17) | 3.20 (0.17) | 8.74 (0.39) | 4.92 (0.24) |
| agg-revenue-by-region | 0.17 (0.08) | 0.76 (0.18) | 4.02 (0.10) | 3.04 (0.09) | 8.86 (0.24) | 5.25 (0.13) |
| agg-top-customers | 0.19 (0.10) | 0.79 (0.20) | 3.98 (0.12) | 3.20 (0.11) | 10.00 (0.30) | 4.88 (0.14) |
| logs-access-top-ips | 1.95 (1.85) | 5.83 (5.20) | 5.20 (1.14) | 4.38 (0.97) | 12.32 (2.62) | 6.46 (1.30) |
| logs-errors-per-service | 1.55 (1.41) | 4.08 (3.38) | 4.52 (0.70) | 3.58 (0.58) | 10.45 (1.73) | 5.75 (0.96) |
| neg-fanout-40 | 0.53 (0.43) | 1.65 (1.01) | 4.76 (0.54) | 3.41 (0.42) | 10.60 (1.74) | 5.55 (0.79) |
| page-accounts-offset | 0.45 (0.35) | 1.55 (0.91) | 3.96 (0.28) | 3.66 (0.27) | 9.23 (0.68) | 5.73 (0.55) |
| page-all-pending | 0.33 (0.23) | 1.14 (0.53) | 3.91 (0.18) | 3.16 (0.17) | 9.38 (0.47) | 4.83 (0.23) |
| page-feb-orders | 0.58 (0.49) | 2.21 (1.62) | 3.85 (0.21) | 3.22 (0.17) | 9.29 (0.49) | 5.52 (0.28) |
| page-open-bugs | 0.22 (0.13) | 0.82 (0.26) | 3.78 (0.12) | 3.17 (0.11) | 8.98 (0.27) | 4.79 (0.16) |
| par-two-level-fanout | 0.72 (0.61) | 2.41 (1.72) | 4.00 (0.34) | 3.36 (0.32) | 10.19 (1.13) | 5.64 (0.71) |
| par-weather-warmest | 0.31 (0.21) | 1.33 (0.69) | 3.99 (0.18) | 3.13 (0.15) | 9.02 (0.46) | 4.88 (0.22) |
| rep-quotes-csv | 0.27 (0.18) | 0.96 (0.39) | 3.82 (0.16) | 3.18 (0.16) | 8.96 (0.47) | 4.87 (0.24) |
| reshape-nested-counts | 0.78 (0.67) | 2.36 (1.79) | 3.84 (0.29) | 3.31 (0.27) | 9.36 (0.66) | 5.68 (0.58) |
| retry-backoff | 0.23 (0.14) | 0.84 (0.28) | 3.77 (0.12) | 3.00 (0.10) | 8.67 (0.32) | 4.84 (0.18) |
| retry-gather-flaky | 0.44 (0.33) | 1.16 (0.60) | 3.94 (0.33) | 3.31 (0.33) | 9.63 (0.92) | 5.72 (0.67) |
| retry-give-up | 0.38 (0.29) | 1.07 (0.51) | 3.74 (0.22) | 3.13 (0.21) | 8.90 (0.65) | 4.95 (0.33) |
| retry-permanent-errors | 0.32 (0.22) | 0.98 (0.41) | 3.76 (0.18) | 3.16 (0.17) | 9.06 (0.47) | 4.92 (0.26) |
| stat-max-drawdown | 0.54 (0.43) | 1.52 (0.95) | 4.05 (0.43) | 3.29 (0.41) | 9.46 (1.18) | 5.18 (0.61) |
| wf-close-fixed | 0.65 (0.51) | 1.79 (1.05) | 3.93 (0.38) | 3.29 (0.33) | 9.65 (1.11) | 5.28 (0.65) |
| wf-customer-latest-order | 0.44 (0.31) | 1.45 (0.66) | 4.37 (0.26) | 3.58 (0.23) | 8.97 (0.53) | 5.46 (0.30) |
| **sum, total** | 12.5 | 38.4 | 89.4 | 73.6 | 210.3 | 116.6 |
| **sum, setup** | 2.3 | 13.8 | 81.9 | 67.1 | 191.1 | 106.0 |
| **sum, run** | 10.2 | 24.7 | 7.2 | 6.4 | 18.9 | 10.5 |

(run only in parentheses)

## Bench programs in-process (ms per run)

| Program | mm-native | mm-wasm | rquickjs | dquickts | quickjs-wasi | djs | djs-wasm |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 0.0 | 0.1 | 0.1 | 0.1 | 0.3 | 0.0 | 0.2 |
| fib | 17.2 | 35.4 | 39.6 | 13.2 | 46.0 | 24.1 | 52.8 |
| loop | 51.4 | 110.2 | 119.2 | 52.8 | 239.3 | 123.4 | 172.2 |
| dict | 85.2 | 146.4 | 104.1 | 76.2 | 208.0 | 111.5 | 220.3 |
| str | 32.3 | 63.2 | 37.5 | 21.2 | 89.0 | 33.0 | 62.9 |
| json_rows | 83.3 | 172.4 | 184.1 | 138.0 | 449.2 | 98.7 | 197.9 |
| closure | 35.3 | 74.3 | 66.2 | 28.5 | 119.0 | 62.8 | 108.8 |
| classes | 71.4 | 139.6 | 101.6 | 61.0 | 218.9 | 96.6 | 179.5 |
| generators | 130.1 | 253.5 | 140.4 | 84.3 | 281.8 | 803.0 | 1747.0 |
| exceptions | 67.5 | 146.1 | 259.8 | 44.2 | 710.6 | 279.4 | 729.0 |
| fmt | 128.6 | 253.6 | 89.8 | 60.4 | 206.3 | 99.0 | 259.4 |
| sort | 104.9 | 179.9 | 283.9 | 154.6 | 394.4 | 289.4 | 514.4 |
| interp | 144.0 | 312.4 | 185.3 | 130.5 | 450.4 | 213.2 | 465.6 |

## Bench programs as CLI processes (ms, spawn to exit)

| Program | qjs-ng | qjs-bellard | djs-cli | mm-cli |
|---|---:|---:|---:|---:|
| startup | 2.7 | 2.8 | 3.0 | 2.7 |
| fib | 16.4 | 15.0 | 29.3 | 18.7 |
| loop | 65.5 | 54.1 | 116.3 | 61.4 |
| dict | 88.5 | 84.1 | 116.2 | 82.4 |
| str | 37.7 | 27.4 | 33.6 | 36.0 |
| json_rows | 170.9 | 151.5 | 102.7 | 84.4 |
| closure | 36.1 | 31.5 | 62.8 | 35.2 |
| classes | 62.4 | 60.3 | 95.6 | 75.6 |
| generators | 68.2 | 80.0 | 847.2 | 136.5 |
| exceptions | 219.5 | 124.1 | 338.8 | 69.1 |
| fmt | 75.4 | 64.6 | 111.5 | 131.9 |
| sort | 167.2 | 162.8 | 296.6 | 112.6 |
| interp | 140.9 | 135.2 | 231.3 | 142.9 |

## Interrupt polling / fuel slices (overhead on the plain run)

| Engine | loop | fib | sort |
|---|---:|---:|---:|
| mm-native | +0.1% | +4.8% | +0.6% |
| mm-wasm | -0.9% | +3.2% | +0.1% |
| rquickjs | -1.5% | -0.6% | +1.5% |
| dquickts | +0.0% | +3.9% | -2.1% |
| quickjs-wasi | +3.1% | +3.5% | +0.3% |
| djs | +4.5% | +8.3% | -2.0% |
| djs-wasm | -4.0% | -0.7% | +1.4% |

## Control (ms to regain control after the stop request; outcome)

| Engine | loop | sort | string | sort-loop | string-loop |
|---|---|---|---|---|---|
| mm-native | 0.04 ms, parked (resumable, instance kept, n=5) | 0.70 ms, parked (resumable, instance kept, n=5) | 0.29 ms, parked (resumable, instance kept, n=5) | 0.53 ms, parked (resumable, instance kept, n=5) | 0.37 ms, parked (resumable, instance kept, n=5) |
| mm-wasm | 0.05 ms, parked (resumable, instance kept, n=5) | 3.00 ms, parked (resumable, instance kept, n=5) | 0.34 ms, parked (resumable, instance kept, n=5) | 1.27 ms, parked (resumable, instance kept, n=5) | 1.09 ms, parked (resumable, instance kept, n=5) |
| rquickjs | 0.08 ms, interrupted (not resumable, instance kept, n=5) | 319.58 ms, interrupted (not resumable, instance kept, n=5) | 35.28 ms, interrupted (not resumable, instance kept, n=5) | not regained in 10 s (n=5) | not regained in 10 s (n=5) |
| dquickts | 0.13 ms, interrupted (not resumable, instance kept, n=5) | 250.70 ms, interrupted (not resumable, instance kept, n=5) | 30.58 ms, interrupted (not resumable, instance kept, n=5) | not regained in 10 s (n=5) | not regained in 10 s (n=5) |
| quickjs-wasi | 0.23 ms, interrupted (not resumable, instance kept, n=5) | 106.70 ms, epoch trap (instance lost) (not resumable, instance lost, n=5) | 106.56 ms, epoch trap (instance lost) (not resumable, instance lost, n=5) | 104.56 ms, epoch trap (instance lost) (not resumable, instance lost, n=5) | 106.66 ms, epoch trap (instance lost) (not resumable, instance lost, n=5) |
| djs | 0.04 ms, stopped at a slice (resumable, instance kept, n=5) | 225.65 ms, stopped at a slice (resumable, instance kept, n=5) | 40.48 ms, stopped at a slice (resumable, instance kept, n=5) | 220.40 ms, stopped at a slice (resumable, instance kept, n=5) | not regained in 10 s (n=5) |

## Snapshots mid-await

| Engine | Point | Kind | Raw bytes | zstd-1 bytes | Snapshot µs | Restore µs | Finished |
|---|---|---|---:|---:|---:|---:|---|
| mm-native | neg-fanout-40@0 | full image (mid-await) | 69,535 | 16,249 | 56.3 | 77.1 | ok |
| mm-native | agg-avg-order-by-tier@3 | full image (mid-await) | 144,586 | 26,655 | 117.3 | 138.7 | ok |
| mm-native | par-two-level-fanout@1 | full image (mid-await) | 170,197 | 39,699 | 143.6 | 209.1 | ok |
| mm-wasm | neg-fanout-40@0 | full image (mid-await) | 73,239 | 18,216 | 224.0 | 358.6 | ok |
| mm-wasm | agg-avg-order-by-tier@3 | full image (mid-await) | 149,652 | 29,008 | 373.5 | 564.0 | ok |
| mm-wasm | par-two-level-fanout@1 | full image (mid-await) | 174,944 | 41,955 | 432.3 | 1187.4 | ok |
| dquickts | neg-fanout-40@0 | heap snapshot (JS_WriteSnapshot, mid-await) | 68,406 | 29,115 | 345.7 | 405.1 | ok |
| dquickts | agg-avg-order-by-tier@3 | heap snapshot (JS_WriteSnapshot, mid-await) | 68,799 | 30,934 | 337.7 | 381.5 | ok |
| dquickts | par-two-level-fanout@1 | heap snapshot (JS_WriteSnapshot, mid-await) | 63,681 | 28,494 | 331.0 | 339.9 | ok |
| quickjs-wasi | neg-fanout-40@0 | linear memory (QuickJS.snapshot, mid-await) | 1,638,440 | 209,131 | 60.5 | 638.0 | ok |
| quickjs-wasi | agg-avg-order-by-tier@3 | linear memory (QuickJS.snapshot, mid-await) | 1,638,440 | 208,677 | 429.2 | 614.2 | ok |
| quickjs-wasi | par-two-level-fanout@1 | linear memory (QuickJS.snapshot, mid-await) | 1,638,440 | 205,717 | 424.5 | 591.1 | ok |
| djs | neg-fanout-40@0 | raw heap image (Vm::image, in-process format) | 397,856 | 103,896 | 10.5 | 236.7 | ok |
| djs | agg-avg-order-by-tier@3 | raw heap image (Vm::image, in-process format) | 398,184 | 107,543 | 8.9 | 227.8 | ok |
| djs | par-two-level-fanout@1 | raw heap image (Vm::image, in-process format) | 761,784 | 142,724 | 19.9 | 602.1 | ok |

## GC (live pairs, 1 GiB limit)

Full: one explicit full collection, ms. Workloads: the longest gap between the
engine's control points (10k-fuel slices, QuickJS's interrupt polls), median of the
rounds' maxima (worst round in brackets), ms; for mm (park requests), the longest
latency from a park request (every 1 ms) to the park. Then the median gap in µs.

| Engine | Items | Live heap MB | Full | garbage | cycles | writes | appends | median gap µs |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| mm-native | 550,000 | 73 | 14.4 | 22.1 (23.8) | 21.7 (22.8) | 24.2 (26.6) | 1.6 (1.7) | 455 |
| mm-native | 2,000,000 | 249 | 74.0 | 24.7 (26.8) | 1.0 (2.0) | 42.0 (51.8) | 1.6 (1.7) | 493 |
| mm-native (park requests) | 550,000 | 73 | 15.5 | 0.9 (0.9) | 3.0 (3.4) | 1.4 (1.8) | 0.2 (0.7) | 14 |
| mm-native (park requests) | 2,000,000 | 249 | 72.7 | 0.1 (0.1) | 0.1 (0.1) | 2.9 (3.3) | 0.5 (0.8) | 15 |
| rquickjs | 550,000 | 123 | 14.8 | 0.7 (1.3) | 61.3 (71.9) | 0.7 (1.5) | 57.6 (64.4) | 395 |
| rquickjs | 2,000,000 | 443 | 135.5 | 0.6 (0.6) | 218.4 (239.6) | 0.6 (0.6) | 229.9 (236.1) | 345 |
| dquickts | 550,000 | 106 | 15.1 | 0.3 (0.5) | 47.1 (61.1) | 0.4 (0.5) | 50.5 (62.5) | 232 |
| dquickts | 2,000,000 | 381 | 127.1 | 0.3 (0.4) | 211.0 (212.7) | 0.4 (1.2) | 1.2 (1.6) | 210 |
| djs | 550,000 | 69 | 28.4 | 34.6 (36.7) | 36.1 (49.7) | 48.1 (49.6) | 60.0 (75.2) | 51 |
| djs | 2,000,000 | 195 | 138.6 | 145.0 (151.9) | 145.4 (153.0) | 189.0 (206.0) | 0.6 (0.6) | 60 |
