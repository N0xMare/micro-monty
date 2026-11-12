# Engine comparison: medians

## Size (bytes; gzip -9)

| Engine | Target | Bytes | gzip -9 | Build |
|---|---|---:|---:|---|
| mm | native | 2,275,864 | 1,341,190 | micro-monty `hello` (cargo xtask size) |
| rquickjs | native | 1,192,376 | 571,752 | size/rquickjs (build.sh) |
| djs | native | 1,600,440 | 775,389 | size/djs (build.sh) |
| qjs-ng | native | 1,269,280 | 615,816 | quickjs-ng qjs CLI (cmake Release, IPO, stripped) |
| qjs-bellard | native | 1,088,840 | 513,390 | QuickJS qjs CLI (make CONFIG_LTO=y, stripped) |
| djs-cli | native | 5,319,624 | 2,208,103 | djs CLI (upstream release profile, stripped) |
| mm | wasm32-unknown-unknown | 2,092,164 | 1,033,293 | micro-monty `hello.wasm` (cargo xtask size) |
| mm-wasm guest | wasm32-unknown-unknown | 2,177,852 | 1,066,806 | mm-wasm v1 guest (cargo xtask size) |
| quickjs-wasi | wasm32-wasip1 | 637,405 | 286,642 | quickjs-wasi 3.6.2 npm module |
| djs-wasm | wasm32-unknown-unknown | 929,693 | 421,721 | djs scripts/build-wasm.sh, then wasm-opt -Oz |

## Start-up (µs)

| Engine | New runtime | New session | Codemode setup | First cell | From image | First cell after image |
|---|---:|---:|---:|---:|---:|---:|
| mm-native | 28.4 | 65.9 | 86.3 | 40.6 | 231.3 | 63.8 |
| mm-wasm | 4553.9 | 243.1 | 609.2 | 101.1 | 1260.1 | 155.9 |
| rquickjs | 0.1 | 173.0 | 5607.7 | 49.1 | — | — |
| dquickts | 0.1 | 152.9 | 4674.5 | 47.6 | 524.3 | 50.1 |
| quickjs-wasi | 4358.3 | 509.6 | 15428.3 | 118.6 | 1029.5 | 148.1 |
| djs | 0.1 | 24.4 | 8177.5 | 73.5 | 74.9 | 51.1 |
| djs-wasm | 6082.2 | 202.2 | 17206.7 | 41.6 | — | — |

## Memory (KB added by N live sessions: RSS / malloc heap / engine count)

| Engine | empty 1 | empty 8 | empty 40 | setup 1 | setup 8 | setup 40 |
|---|---:|---:|---:|---:|---:|---:|
| mm-native | 0 / 113 / 0 | 1,339 / 898 / 0 | 3,625 / 4,572 / 0 | 0 / 125 / 0 | 2,347 / 1,006 / 0 | 5,239 / 5,061 / 0 |
| mm-wasm | 0 / 31 / 1,769 | 1,634 / 266 / 14,156 | 9,691 / 1,415 / 70,779 | 0 / 33 / 1,769 | 3,371 / 273 / 14,156 | 19,059 / 1,426 / 70,779 |
| rquickjs | 0 / 164 / 107 | 1,757 / 1,342 / 859 | 7,180 / 6,739 / 4,294 | 131 / 448 / 363 | 3,207 / 3,575 / 2,903 | 17,625 / 17,802 / 14,516 |
| dquickts | 0 / 162 / 165 | 1,757 / 1,323 / 1,322 | 7,180 / 6,647 / 6,607 | 0 / 472 / 467 | 3,211 / 3,774 / 3,740 | 18,461 / 18,785 / 18,698 |
| quickjs-wasi | 0 / 42 / 1,376 | 2,437 / 352 / 11,010 | 12,751 / 1,794 / 55,050 | 369 / 43 / 1,638 | 4,899 / 356 / 13,107 | 24,797 / 1,798 / 65,536 |
| djs | 0 / 263 / 200 | 1,618 / 2,100 / 1,597 | 8,647 / 10,508 / 7,985 | 807 / 810 / 738 | 6,214 / 6,376 / 5,904 | 30,544 / 31,631 / 29,522 |
| djs-wasm | 0 / 12 / 590 | 2,273 / 94 / 4,719 | 12,464 / 507 / 23,593 | 1,159 / 11 / 1,573 | 10,953 / 94 / 12,583 | 55,067 / 507 / 62,915 |

## Host calls (µs per call: tool minus local, and total)

| Engine | Sync | Sync total | Awaited | Awaited total | Fan-out of 100 | Fan-out total |
|---|---:|---:|---:|---:|---:|---:|
| mm-native | 0.39 | 0.47 | 3.34 | 3.77 | 3.00 | 3.77 |
| mm-wasm | 1.04 | 1.18 | 7.75 | 8.49 | 6.78 | 8.39 |
| rquickjs | 0.08 | 0.15 | 4.21 | 5.51 | 5.21 | 7.44 |
| dquickts | 0.05 | 0.10 | 3.26 | 4.35 | 3.83 | 5.74 |
| quickjs-wasi | 2.24 | 2.48 | 29.83 | 32.71 | 29.92 | 34.87 |
| djs | 0.28 | 0.37 | 8.91 | 10.24 | 8.13 | 11.76 |

## Codemode workload (ms per task: new session + setup + run)

| Task | mm-native | mm-wasm | rquickjs | dquickts | quickjs-wasi | djs |
|---|---:|---:|---:|---:|---:|---:|
| agg-avg-order-by-tier | 1.91 (1.70) | 4.27 (3.24) | 7.38 (1.32) | 6.31 (1.12) | 18.77 (3.36) | 10.44 (1.69) |
| agg-customer-summary | 0.54 (0.35) | 1.68 (0.67) | 5.92 (0.27) | 5.35 (0.28) | 16.69 (0.63) | 9.14 (0.40) |
| agg-revenue-by-region | 0.35 (0.16) | 1.20 (0.31) | 5.96 (0.17) | 5.20 (0.16) | 16.66 (0.39) | 8.07 (0.21) |
| agg-top-customers | 0.29 (0.14) | 1.16 (0.32) | 5.81 (0.19) | 5.12 (0.16) | 13.98 (0.41) | 7.99 (0.21) |
| logs-access-top-ips | 4.23 (4.06) | 9.75 (8.96) | 7.34 (1.71) | 6.25 (1.49) | 16.84 (3.67) | 9.38 (1.99) |
| logs-errors-per-service | 3.97 (3.80) | 10.24 (9.17) | 7.75 (1.26) | 6.47 (1.04) | 17.14 (2.76) | 9.06 (1.59) |
| neg-fanout-40 | 0.96 (0.76) | 2.45 (1.58) | 7.10 (0.81) | 5.88 (0.75) | 17.45 (2.54) | 9.96 (1.48) |
| page-accounts-offset | 0.81 (0.61) | 2.21 (1.27) | 6.86 (0.46) | 5.95 (0.42) | 16.29 (1.13) | 9.60 (0.99) |
| page-all-pending | 0.52 (0.36) | 1.82 (0.87) | 6.30 (0.30) | 5.49 (0.28) | 14.96 (0.72) | 8.73 (0.39) |
| page-feb-orders | 0.99 (0.80) | 3.28 (2.41) | 6.07 (0.34) | 5.01 (0.28) | 13.99 (0.77) | 8.24 (0.40) |
| page-open-bugs | 0.39 (0.24) | 1.36 (0.48) | 6.54 (0.21) | 4.82 (0.19) | 17.24 (0.44) | 7.89 (0.27) |
| par-two-level-fanout | 1.18 (1.00) | 3.62 (2.75) | 6.14 (0.55) | 5.21 (0.51) | 16.11 (1.72) | 9.31 (1.25) |
| par-weather-warmest | 0.50 (0.32) | 1.62 (0.79) | 5.83 (0.25) | 5.42 (0.24) | 14.09 (0.64) | 8.75 (0.35) |
| rep-quotes-csv | 0.45 (0.30) | 1.39 (0.61) | 6.03 (0.27) | 5.15 (0.26) | 13.97 (0.68) | 8.13 (0.37) |
| reshape-nested-counts | 1.25 (1.05) | 3.65 (2.75) | 6.03 (0.44) | 5.15 (0.39) | 15.28 (1.01) | 8.86 (0.95) |
| retry-backoff | 0.42 (0.26) | 1.25 (0.45) | 5.67 (0.18) | 5.00 (0.18) | 14.25 (0.50) | 7.94 (0.28) |
| retry-gather-flaky | 0.72 (0.54) | 1.98 (1.02) | 6.08 (0.53) | 5.45 (0.54) | 16.02 (1.48) | 9.42 (1.23) |
| retry-give-up | 0.68 (0.48) | 1.65 (0.82) | 5.81 (0.35) | 4.96 (0.30) | 15.23 (0.96) | 8.06 (0.53) |
| retry-permanent-errors | 0.58 (0.39) | 1.60 (0.67) | 5.84 (0.30) | 5.15 (0.29) | 14.60 (0.71) | 8.26 (0.42) |
| stat-max-drawdown | 0.91 (0.72) | 2.59 (1.62) | 6.55 (0.68) | 5.58 (0.64) | 15.85 (1.87) | 8.69 (1.09) |
| wf-close-fixed | 1.09 (0.86) | 3.11 (1.85) | 6.79 (0.59) | 5.80 (0.58) | 15.28 (1.56) | 8.66 (1.13) |
| wf-customer-latest-order | 0.77 (0.51) | 2.30 (1.09) | 6.64 (0.36) | 5.81 (0.36) | 15.28 (0.90) | 8.96 (0.47) |
| **sum, total** | 23.5 | 64.2 | 140.5 | 120.5 | 346.0 | 193.6 |
| **sum, setup** | 3.8 | 20.1 | 128.9 | 110.0 | 316.9 | 175.0 |
| **sum, run** | 19.4 | 43.7 | 11.5 | 10.5 | 28.8 | 17.7 |

(run only in parentheses)

## Bench programs in-process (ms per run)

| Program | mm-native | mm-wasm | rquickjs | dquickts | quickjs-wasi | djs | djs-wasm |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 0.1 | 0.3 | 0.2 | 0.2 | 0.6 | 0.0 | 0.4 |
| fib | 36.2 | 66.7 | 33.5 | 26.6 | 90.9 | 54.0 | 96.1 |
| loop | 124.4 | 232.9 | 132.0 | 81.7 | 512.5 | 247.3 | 396.5 |
| dict | 171.3 | 327.2 | 214.7 | 174.3 | 418.1 | 276.5 | 465.0 |
| str | 61.3 | 109.5 | 57.3 | 37.1 | 140.8 | 60.0 | 100.7 |
| json_rows | 139.7 | 289.7 | 304.7 | 251.1 | 689.1 | 155.3 | 353.4 |
| closure | 69.6 | 129.9 | 72.9 | 48.0 | 217.8 | 145.8 | 212.8 |
| classes | 142.1 | 250.9 | 122.4 | 94.4 | 410.0 | 180.7 | 298.6 |
| generators | 252.7 | 499.5 | 170.1 | 158.0 | 464.1 | 1522.6 | 3249.3 |
| exceptions | 132.8 | 259.1 | 413.5 | 70.5 | 1319.2 | 508.0 | 1278.5 |
| fmt | 251.2 | 450.1 | 143.2 | 106.4 | 381.1 | 202.9 | 472.7 |
| sort | 240.8 | 436.9 | 493.0 | 433.9 | 861.8 | 655.1 | 1099.8 |
| interp | 295.9 | 629.0 | 332.9 | 252.7 | 806.8 | 463.6 | 835.1 |

## Bench programs as CLI processes (ms, spawn to exit)

| Program | qjs-ng | qjs-bellard | djs-cli | mm-cli |
|---|---:|---:|---:|---:|
| startup | 4.6 | 6.8 | 4.9 | 5.1 |
| fib | 33.5 | 33.1 | 66.2 | 49.7 |
| loop | 93.9 | 93.0 | 265.8 | 132.8 |
| dict | 177.8 | 159.6 | 245.7 | 205.0 |
| str | 59.2 | 50.0 | 59.6 | 67.0 |
| json_rows | 322.4 | 253.4 | 171.0 | 144.8 |
| closure | 60.0 | 53.5 | 137.5 | 73.3 |
| classes | 123.9 | 101.9 | 220.4 | 167.0 |
| generators | 155.2 | 146.8 | 1685.1 | 279.0 |
| exceptions | 395.2 | 254.6 | 653.0 | 139.6 |
| fmt | 127.7 | 109.4 | 213.3 | 234.1 |
| sort | 423.4 | 383.8 | 634.2 | 270.7 |
| interp | 303.7 | 243.5 | 477.4 | 314.3 |

## Interrupt polling / fuel slices (overhead on the plain run)

| Engine | loop | fib | sort |
|---|---:|---:|---:|
| mm-native | -1.6% | +0.8% | -6.7% |
| mm-wasm | -3.2% | +11.2% | +3.9% |
| rquickjs | -5.4% | +0.1% | +3.7% |
| dquickts | +1.4% | +0.3% | -1.5% |
| quickjs-wasi | +1.8% | +3.4% | +8.7% |
| djs | +1.1% | +4.0% | +0.0% |
| djs-wasm | -1.3% | +4.5% | -1.9% |

## Control (ms to regain control after the stop request; outcome)

| Engine | loop | sort | string | sort-loop | string-loop |
|---|---|---|---|---|---|
| mm-native | 0.08 ms, parked (resumable, instance kept, n=5) | 2658.93 ms, parked (resumable, instance kept, n=5) | 126.36 ms, parked (resumable, instance kept, n=5) | 2356.01 ms, parked (resumable, instance kept, n=5) | 23.67 ms, parked (resumable, instance kept, n=5) |
| mm-wasm | 0.09 ms, parked (resumable, instance kept, n=5) | 104.17 ms, trap: instance trapped: wasm trap: interrupt (not resumable, instance lost, n=5) | 126.21 ms, trap: instance trapped: wasm trap: interrupt (not resumable, instance lost, n=5) | 104.95 ms, trap: instance trapped: wasm trap: interrupt (not resumable, instance lost, n=5) | 38.64 ms, parked (resumable, instance kept, n=5) |
| rquickjs | 0.20 ms, interrupted (not resumable, instance kept, n=5) | 731.82 ms, interrupted (not resumable, instance kept, n=5) | 58.63 ms, interrupted (not resumable, instance kept, n=5) | not regained in 10 s (n=5) | not regained in 10 s (n=5) |
| dquickts | 0.20 ms, interrupted (not resumable, instance kept, n=5) | 654.59 ms, interrupted (not resumable, instance kept, n=5) | 49.81 ms, interrupted (not resumable, instance kept, n=5) | not regained in 10 s (n=5) | not regained in 10 s (n=5) |
| quickjs-wasi | 0.48 ms, interrupted (not resumable, instance kept, n=5) | 107.40 ms, epoch trap (instance lost) (not resumable, instance lost, n=5) | 126.32 ms, epoch trap (instance lost) (not resumable, instance lost, n=5) | 102.97 ms, epoch trap (instance lost) (not resumable, instance lost, n=5) | 106.02 ms, epoch trap (instance lost) (not resumable, instance lost, n=5) |
| djs | 0.10 ms, stopped at a slice (resumable, instance kept, n=5) | 30.42 ms, stopped at a slice (resumable, instance kept, n=5) | 58.02 ms, stopped at a slice (resumable, instance kept, n=5) | 18.87 ms, stopped at a slice (resumable, instance kept, n=5) | not regained in 10 s (n=5) |

## Snapshots mid-await

| Engine | Point | Kind | Raw bytes | zstd-1 bytes | Snapshot µs | Restore µs | Finished |
|---|---|---|---:|---:|---:|---:|---|
| mm-native | neg-fanout-40@0 | full image (mid-await) | 69,158 | 16,062 | 61.3 | 119.5 | ok |
| mm-native | agg-avg-order-by-tier@3 | full image (mid-await) | 144,257 | 26,368 | 127.6 | 251.1 | ok |
| mm-native | par-two-level-fanout@1 | full image (mid-await) | 169,342 | 39,352 | 164.9 | 347.2 | ok |
| mm-wasm | neg-fanout-40@0 | full image (mid-await) | 72,870 | 18,030 | 213.9 | 1171.3 | ok |
| mm-wasm | agg-avg-order-by-tier@3 | full image (mid-await) | 149,331 | 28,741 | 382.9 | 1470.9 | ok |
| mm-wasm | par-two-level-fanout@1 | full image (mid-await) | 174,097 | 41,605 | 472.7 | 2235.3 | ok |
| dquickts | neg-fanout-40@0 | heap snapshot (JS_WriteSnapshot, mid-await) | 68,406 | 29,117 | 643.4 | 625.9 | ok |
| dquickts | agg-avg-order-by-tier@3 | heap snapshot (JS_WriteSnapshot, mid-await) | 68,799 | 30,937 | 635.0 | 641.6 | ok |
| dquickts | par-two-level-fanout@1 | heap snapshot (JS_WriteSnapshot, mid-await) | 63,681 | 28,495 | 499.8 | 575.2 | ok |
| quickjs-wasi | neg-fanout-40@0 | linear memory (QuickJS.snapshot, mid-await) | 1,638,440 | 209,114 | 117.6 | 1024.1 | ok |
| quickjs-wasi | agg-avg-order-by-tier@3 | linear memory (QuickJS.snapshot, mid-await) | 1,638,440 | 209,069 | 637.6 | 977.0 | ok |
| quickjs-wasi | par-two-level-fanout@1 | linear memory (QuickJS.snapshot, mid-await) | 1,638,440 | 205,755 | 515.7 | 992.1 | ok |
| djs | neg-fanout-40@0 | raw heap image (Vm::image, in-process format) | 397,856 | 103,896 | 11.5 | 422.0 | ok |
| djs | agg-avg-order-by-tier@3 | raw heap image (Vm::image, in-process format) | 398,184 | 107,543 | 9.7 | 379.8 | ok |
| djs | par-two-level-fanout@1 | raw heap image (Vm::image, in-process format) | 761,784 | 142,724 | 26.7 | 1115.5 | ok |
