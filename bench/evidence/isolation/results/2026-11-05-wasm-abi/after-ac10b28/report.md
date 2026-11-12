## mm-wasm guest size (shipped build) [M]

- wasm32-wasip1 mm_wasm_guest.wasm 2111851 bytes, 1926756 after wasm-opt -Oz
- wasm32-unknown-unknown mm_wasm_guest.wasm 2205342 bytes, 2015061 after wasm-opt -Oz

## Startup [M]

| cold: mode | main → first cell done (ms) | module load (ms) | spawn → exit (ms) |
|---|---|---|---|
| native | 0.29 | 0.00 | 2.11 |
| wasm | 1.45 | 0.37 | 3.76 |
| wasm-compile | 285 | 284 | 299 |
| abi | 1.19 | 0.37 | 3.34 |
| abi-uu | 1.25 | 0.35 | 3.28 |
| child | 2.00 | 0.00 | 4.02 |

| warm: mode | what | session ready (µs) | first cell done (µs, median) | p99 (µs) |
|---|---|---|---|---|
| native | new container + session | 39.3 | 42.9 | 111 |
| wasm | new container + session | 158 | 176 | 269 |
| wasm-pool | new container + session | 156 | 173 | 249 |
| abi | new container + session | 167 | 181 | 397 |
| abi-uu | new container + session | 161 | 176 | 278 |
| child | new container + session | 1,784 | 1,937 | 2,644 |
| native | new session in an existing container | — | 43.2 | 108 |
| wasm | new session in an existing container | — | 107 | 181 |
| child | new session in an existing container | — | 150 | 236 |

## Host calls [M]

| mode | host call round trip (µs) | loop with host call (µs/iter) | loop with Python call (µs/iter) | codemode cell, 400 calls (ms) | codemode µs per call |
|---|---|---|---|---|---|
| native | 0.22 | 0.27 | 0.05 | 1.99 | 4.98 |
| wasm | 2.54 | 2.65 | 0.11 | 5.53 | 13.82 |
| abi | 1.02 | 1.12 | 0.10 | 4.42 | 11.06 |
| abi-uu | 0.98 | 1.08 | 0.10 | 4.31 | 10.78 |
| child | 54.30 | 54.35 | 0.06 | 27.98 | 69.96 |
| abi-uu-opt | 0.89 | 0.99 | 0.10 | 4.28 | 10.69 |

## Bench programs, ms per run (median) [M]

| program | native | wasm | wasm-plain | wasm-fuel | abi | abi-uu | child | abi-uu-opt |
|---|---|---|---|---|---|---|---|---|
| startup | 0.01 | 0.03 | 0.02 | 0.06 | 0.02 | 0.03 | 0.14 | 0.02 |
| fib | 26.60 | 54.22 | 49.30 | 50.52 | 54.15 | 55.68 | 22.68 | 52.80 |
| loop | 75.37 | 151 | 124 | 140 | 152 | 148 | 67.90 | 150 |
| dict | 120 | 242 | 203 | 256 | 251 | 233 | 118 | 240 |
| str | 46.25 | 91.75 | 77.59 | 97.60 | 90.43 | 88.99 | 48.22 | 92.87 |
| json_rows | 120 | 230 | 199 | 259 | 234 | 230 | 110 | 234 |
| closure | 48.08 | 97.91 | 77.80 | 97.53 | 98.71 | 99.73 | 45.80 | 98.96 |
| classes | 140 | 334 | 282 | 359 | 321 | 309 | 144 | 281 |
| generators | 281 | 625 | 523 | 733 | 624 | 615 | 286 | 614 |
| exceptions | 97.14 | 208 | 179 | 252 | 211 | 217 | 97.82 | 206 |
| fmt | 225 | 469 | 389 | 527 | 470 | 446 | 258 | 457 |
| sort | 160 | 315 | 272 | 365 | 324 | 314 | 158 | 351 |
| interp | 261 | 583 | 481 | 651 | 599 | 578 | 271 | 554 |
| **geomean vs native** (excl. startup) | 1.00× | 2.08× | 1.76× | 2.24× | 2.10× | 2.05× | 0.99× | 2.05× |

## Snapshots [M]

| state | mode | bytes | snapshot (µs) | restore (µs) |
|---|---|---|---|---|
| codemode@call100 | native | 56,001 | 58.5 | 115 |
| dicts10k@call | native | 1,891,570 | 1,894 | 999 |
| dicts100k@call | native | 17,999,715 | 26,567 | 21,630 |
| codemode@call100 | wasm | 56,001 | 635 | 708 |
| dicts10k@call | wasm | 1,891,570 | 5,145 | 3,866 |
| dicts100k@call | wasm | 17,999,715 | 65,038 | 48,662 |
| codemode@call100 | abi | 56,001 | 568 | 738 |
| dicts10k@call | abi | 1,891,570 | 3,792 | 3,526 |
| dicts100k@call | abi | 17,999,715 | 64,273 | 48,791 |
| codemode@call100 | abi-uu | 56,001 | 573 | 712 |
| dicts10k@call | abi-uu | 1,891,570 | 4,185 | 3,797 |
| dicts100k@call | abi-uu | 17,999,715 | 63,727 | 48,595 |
| codemode@call100 | child | 56,001 | 577 | 2,137 |
| dicts10k@call | child | 1,891,570 | 6,099 | 6,865 |
| dicts100k@call | child | 17,999,715 | 73,623 | 50,140 |

| saved in | restored in | result |
|---|---|---|
| native | native | resumed, output identical |
| native | wasm | resumed, output identical |
| native | abi | resumed, output identical |
| native | abi-uu | resumed, output identical |
| native | child | resumed, output identical |
| wasm | native | resumed, output identical |
| wasm | wasm | resumed, output identical |
| wasm | abi | resumed, output identical |
| wasm | abi-uu | resumed, output identical |
| wasm | child | resumed, output identical |
| abi | native | resumed, output identical |
| abi | wasm | resumed, output identical |
| abi | abi | resumed, output identical |
| abi | abi-uu | resumed, output identical |
| abi | child | resumed, output identical |
| abi-uu | native | resumed, output identical |
| abi-uu | wasm | resumed, output identical |
| abi-uu | abi | resumed, output identical |
| abi-uu | abi-uu | resumed, output identical |
| abi-uu | child | resumed, output identical |
| child | native | resumed, output identical |
| child | wasm | resumed, output identical |
| child | abi | resumed, output identical |
| child | abi-uu | resumed, output identical |
| child | child | resumed, output identical |

## Memory per session (100 sessions) [M]

| mode | state | host RSS Δ (KiB/session) | wasm linear memory (KiB) | worker RSS (KiB) | worker PSS (KiB) |
|---|---|---|---|---|---|
| native | idle | 73.9 | — | — | — |
| native | active | 386 | — | — | — |
| wasm | idle | 385 | 1,664 | — | — |
| wasm | active | 1,331 | 2,560 | — | — |
| abi | idle | 349 | 1,600 | — | — |
| abi | active | 1,299 | 2,560 | — | — |
| abi-uu | idle | 345 | 1,728 | — | — |
| abi-uu | active | 1,290 | 2,816 | — | — |
| child | idle | 0.0 | — | 1,048 | 333 |
| child | active | 0.0 | — | 3,444 | 805 |

## Many sessions on one host (codemode cells) [M]

| mode | sessions | threads | cells | errors | cells/s | p50 (ms) | p99 (ms) | host RSS (MiB) | workers PSS (MiB) |
|---|---|---|---|---|---|---|---|---|---|
| native | 1 | 1 | 256 | 0 | 462 | 2.09 | 2.72 | 6.5 | 0.0 |
| native | 16 | 16 | 256 | 0 | 3,995 | 2.32 | 13.44 | 23.1 | 0.0 |
| native | 64 | 16 | 256 | 0 | 2,693 | 3.12 | 14.73 | 61.0 | 0.0 |
| native | 256 | 16 | 256 | 0 | 1,356 | 10.57 | 19.39 | 158 | 0.0 |
| native | 1024 | 16 | 1024 | 0 | 1,373 | 10.60 | 19.19 | 603 | 0.0 |
| wasm | 1 | 1 | 256 | 0 | 168 | 5.69 | 8.09 | 160 | 0.0 |
| wasm | 16 | 16 | 256 | 0 | 1,561 | 6.67 | 14.68 | 165 | 0.0 |
| wasm | 64 | 16 | 256 | 0 | 1,890 | 6.03 | 11.37 | 239 | 0.0 |
| wasm | 256 | 16 | 256 | 0 | 1,371 | 7.69 | 13.41 | 483 | 0.0 |
| wasm | 1024 | 16 | 1024 | 0 | 1,859 | 7.44 | 13.24 | 1,521 | 0.0 |
| abi | 1 | 1 | 256 | 0 | 226 | 4.39 | 4.65 | 137 | 0.0 |
| abi | 16 | 16 | 256 | 0 | 1,832 | 4.77 | 11.14 | 163 | 0.0 |
| abi | 64 | 16 | 256 | 0 | 2,044 | 4.67 | 12.14 | 244 | 0.0 |
| abi | 256 | 16 | 256 | 0 | 1,583 | 8.14 | 13.03 | 498 | 0.0 |
| abi | 1024 | 16 | 1024 | 0 | 1,557 | 8.07 | 14.02 | 1,584 | 0.0 |
| abi-uu | 1 | 1 | 256 | 0 | 228 | 4.32 | 5.36 | 137 | 0.0 |
| abi-uu | 16 | 16 | 256 | 0 | 1,929 | 4.36 | 10.10 | 163 | 0.0 |
| abi-uu | 64 | 16 | 256 | 0 | 2,289 | 4.49 | 10.01 | 243 | 0.0 |
| abi-uu | 256 | 16 | 256 | 0 | 1,467 | 6.71 | 11.86 | 497 | 0.0 |
| abi-uu | 1024 | 16 | 1024 | 0 | 1,778 | 7.00 | 12.26 | 1,580 | 0.0 |
| child | 1 | 1 | 256 | 0 | 37.7 | 25.93 | 31.38 | 133 | 3.6 |
| child | 16 | 16 | 256 | 0 | 641 | 25.02 | 42.00 | 133 | 22.0 |
| child | 64 | 16 | 256 | 0 | 489 | 31.77 | 40.85 | 133 | 77.4 |
| child | 256 | 16 | 256 | 0 | 449 | 35.29 | 40.27 | 133 | 223 |
| child | 1024 | 16 | 1024 | 0 | 431 | 36.47 | 46.60 | 133 | 881 |

## Fault matrix [M]

| scenario | host | victim | victim session after | bystander session | fresh session | time to control |
|---|---|---|---|---|---|---|
| stack-cyclic-dict native thread 256 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 512 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 2048 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 8192 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 512 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict child worker stack 256 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict child worker stack 512 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict child worker stack 16384 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 256 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 512 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 2048 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 8192 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 wasm max_wasm_stack 512 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 wasm max_wasm_stack 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 wasm max_wasm_stack 4096 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 child worker stack 256 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 child worker stack 512 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 child worker stack 16384 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi max_wasm_stack 512 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi max_wasm_stack 512 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi max_wasm_stack 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi max_wasm_stack 4096 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| panic abi release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| runaway loop abi engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 552 ms after start |
| runaway loop abi fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.081 ms after the stop |
| runaway loop abi park at 200 ms (grace 100 ms) | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.020 ms after the stop |
| runaway loop abi call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| runaway native abi engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native abi fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 6384.713 ms after the stop |
| runaway native abi park at 200 ms (grace 100 ms) | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 100.724 ms after the stop |
| runaway native abi call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| memory arena 64 MiB abi | survived | MemoryError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 512 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 512 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 4096 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| panic abi-uu release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| runaway loop abi-uu engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 633 ms after start |
| runaway loop abi-uu fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.074 ms after the stop |
| runaway loop abi-uu park at 200 ms (grace 100 ms) | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.036 ms after the stop |
| runaway loop abi-uu call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| runaway native abi-uu engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native abi-uu fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 6383.614 ms after the stop |
| runaway native abi-uu park at 200 ms (grace 100 ms) | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 100.762 ms after the stop |
| runaway native abi-uu call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 200 ms after start |
| memory arena 64 MiB abi-uu | survived | MemoryError | completed: 2 | completed | ok |  |
| panic abi-uu shipped guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic native release (panic=abort) | **died: killed by SIGABRT** | test hook: injected panic in start; note: run with `RUST_BACKTRACE=1` environment variable | lost with the host | lost with the host | — | — |
| panic native release-unwind | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic native shipped (immediate-abort) | **died: killed by SIGILL** |  | lost with the host | lost with the host | — | — |
| panic wasm release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic wasm shipped guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic child release worker (panic=abort) | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| panic child release-unwind worker | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic child shipped worker | survived | lost: worker exited: signal: 4 (SIGILL) | lost: worker exited: signal: 4 (SIGILL) | completed | ok |  |
| runaway loop native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 322 ms after start |
| runaway loop native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.056 ms after the stop |
| runaway loop native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.014 ms after the stop |
| runaway loop wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 853 ms after start |
| runaway loop wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.052 ms after the stop |
| runaway loop wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.187 ms after the stop |
| runaway loop child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 310 ms after start |
| runaway loop child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.057 ms after the stop |
| runaway loop child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.239 ms after the stop |
| runaway loop wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 209 ms after start |
| runaway loop wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 209 ms after start |
| runaway loop child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.488 ms after the stop |
| runaway loop child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 999 ms after start |
| runaway native native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 2805.758 ms after the stop |
| runaway native native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7331.964 ms after the stop |
| runaway native wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 6365.450 ms after the stop |
| runaway native wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.132 ms after the stop |
| runaway native child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 2865.149 ms after the stop |
| runaway native child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7384.954 ms after the stop |
| runaway native wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| runaway native wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 119 ms after start |
| runaway native child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.607 ms after the stop |
| runaway native child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 999 ms after start |
| memory arena 64 MiB native | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB wasm | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB child | survived | MemoryError | completed: 2 | completed | ok |  |
| memory outside arena (completion value) native | survived | completed:  (host peak RSS 114 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) wasm memory_limit_mib=256 | survived | completed:  (host peak RSS 171 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) abi memory_limit_mib=256 | survived | completed:  (host peak RSS 174 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) abi-uu memory_limit_mib=256 | survived | completed:  (host peak RSS 174 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child | survived | completed:  (host peak RSS 4 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child prlimit=--as=268435456 | survived | completed:  (host peak RSS 4 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) native | survived | MemoryError (host peak RSS 111 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) wasm memory_limit_mib=256 | survived | MemoryError (host peak RSS 117 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) abi memory_limit_mib=256 | survived | MemoryError (host peak RSS 117 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) abi-uu memory_limit_mib=256 | survived | MemoryError (host peak RSS 117 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child | survived | MemoryError (host peak RSS 2 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child prlimit=--as=268435456 | survived | MemoryError (host peak RSS 2 MiB) | completed: 2 | completed | ok |  |

