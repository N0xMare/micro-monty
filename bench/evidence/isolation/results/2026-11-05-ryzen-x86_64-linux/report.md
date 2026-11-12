## mm-wasm guest size (shipped build) [M]

- wasm32-wasip1 mm_wasm_guest.wasm 2111851 bytes, 1926733 after wasm-opt -Oz
- wasm32-unknown-unknown mm_wasm_guest.wasm 2205342 bytes, 2015033 after wasm-opt -Oz

## Startup [M]

| cold: mode | main → first cell done (ms) | module load (ms) | spawn → exit (ms) |
|---|---|---|---|
| native | 0.17 | 0.00 | 1.15 |
| wasm | 0.82 | 0.32 | 1.87 |
| wasm-compile | 180 | 179 | 191 |
| abi | 0.81 | 0.32 | 1.86 |
| abi-uu | 0.64 | 0.27 | 1.77 |
| child | 0.98 | 0.00 | 1.97 |

| warm: mode | what | session ready (µs) | first cell done (µs, median) | p99 (µs) |
|---|---|---|---|---|
| native | new container + session | 22.7 | 24.6 | 32.4 |
| wasm | new container + session | 107 | 116 | 131 |
| wasm-pool | new container + session | 101 | 109 | 133 |
| abi | new container + session | 106 | 112 | 128 |
| abi-uu | new container + session | 106 | 113 | 151 |
| child | new container + session | 789 | 834 | 1,085 |
| native | new session in an existing container | — | 24.6 | 39.6 |
| wasm | new session in an existing container | — | 59.7 | 70.6 |
| child | new session in an existing container | — | 45.1 | 80.8 |

## Host calls [M]

| mode | host call round trip (µs) | loop with host call (µs/iter) | loop with Python call (µs/iter) | codemode cell, 400 calls (ms) | codemode µs per call |
|---|---|---|---|---|---|
| native | 0.13 | 0.15 | 0.03 | 1.07 | 2.69 |
| wasm | 1.45 | 1.51 | 0.06 | 2.86 | 7.15 |
| abi | 0.56 | 0.62 | 0.06 | 2.33 | 5.82 |
| abi-uu | 0.52 | 0.58 | 0.06 | 2.28 | 5.70 |
| child | 10.38 | 10.41 | 0.03 | 5.73 | 14.31 |
| abi-uu-opt | 0.52 | 0.58 | 0.06 | 2.26 | 5.66 |

## Bench programs, ms per run (median) [M]

| program | native | wasm | wasm-plain | wasm-fuel | abi | abi-uu | child | abi-uu-opt |
|---|---|---|---|---|---|---|---|---|
| startup | 0.00 | 0.01 | 0.01 | 0.01 | 0.01 | 0.01 | 0.09 | 0.01 |
| fib | 14.40 | 28.79 | 23.98 | 27.39 | 28.86 | 30.34 | 14.23 | 28.42 |
| loop | 43.86 | 93.15 | 70.88 | 86.29 | 92.28 | 101 | 42.08 | 94.43 |
| dict | 63.00 | 119 | 104 | 122 | 118 | 118 | 65.04 | 117 |
| str | 26.10 | 48.47 | 42.62 | 51.34 | 47.63 | 48.86 | 28.13 | 48.10 |
| json_rows | 60.34 | 122 | 108 | 135 | 122 | 123 | 63.50 | 124 |
| closure | 28.69 | 60.28 | 45.53 | 56.88 | 60.15 | 61.95 | 28.27 | 61.77 |
| classes | 73.17 | 170 | 140 | 188 | 173 | 172 | 74.53 | 159 |
| generators | 155 | 325 | 271 | 328 | 312 | 311 | 155 | 315 |
| exceptions | 51.47 | 110 | 95.88 | 118 | 110 | 112 | 53.43 | 108 |
| fmt | 117 | 225 | 197 | 255 | 227 | 224 | 120 | 220 |
| sort | 80.67 | 162 | 141 | 172 | 162 | 162 | 81.59 | 161 |
| interp | 139 | 306 | 255 | 337 | 308 | 302 | 139 | 301 |
| **geomean vs native** (excl. startup) | 1.00× | 2.05× | 1.73× | 2.14× | 2.05× | 2.08× | 1.02× | 2.03× |

## Snapshots [M]

| state | mode | bytes | snapshot (µs) | restore (µs) |
|---|---|---|---|---|
| codemode@call100 | native | 56,001 | 33.5 | 64.2 |
| dicts10k@call | native | 1,891,570 | 1,281 | 564 |
| dicts100k@call | native | 17,999,715 | 14,378 | 11,429 |
| codemode@call100 | wasm | 56,001 | 442 | 529 |
| dicts10k@call | wasm | 1,891,570 | 2,996 | 2,441 |
| dicts100k@call | wasm | 17,999,715 | 43,012 | 29,446 |
| codemode@call100 | abi | 56,001 | 444 | 541 |
| dicts10k@call | abi | 1,891,570 | 2,350 | 2,455 |
| dicts100k@call | abi | 17,999,715 | 43,032 | 29,336 |
| codemode@call100 | abi-uu | 56,001 | 432 | 521 |
| dicts10k@call | abi-uu | 1,891,570 | 2,606 | 2,444 |
| dicts100k@call | abi-uu | 17,999,715 | 41,300 | 29,185 |
| codemode@call100 | child | 56,001 | 346 | 1,280 |
| dicts10k@call | child | 1,891,570 | 3,266 | 3,858 |
| dicts100k@call | child | 17,999,715 | 45,135 | 24,988 |

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
| native | idle | 69.8 | — | — | — |
| native | active | 385 | — | — | — |
| wasm | idle | 393 | 1,664 | — | — |
| wasm | active | 1,333 | 2,560 | — | — |
| abi | idle | 541 | 1,600 | — | — |
| abi | active | 1,292 | 2,560 | — | — |
| abi-uu | idle | 352 | 1,728 | — | — |
| abi-uu | active | 1,292 | 2,816 | — | — |
| child | idle | 0.0 | — | 4,132 | 337 |
| child | active | 0.0 | — | 5,152 | 809 |

## Many sessions on one host (codemode cells) [M]

| mode | sessions | threads | cells | errors | cells/s | p50 (ms) | p99 (ms) | host RSS (MiB) | workers PSS (MiB) |
|---|---|---|---|---|---|---|---|---|---|
| native | 1 | 1 | 256 | 0 | 918 | 1.08 | 1.34 | 7.4 | 0.0 |
| native | 16 | 16 | 256 | 0 | 6,481 | 2.18 | 5.53 | 23.5 | 0.0 |
| native | 64 | 16 | 256 | 0 | 5,448 | 2.28 | 5.66 | 62.1 | 0.0 |
| native | 256 | 16 | 256 | 0 | 3,275 | 4.73 | 6.30 | 158 | 0.0 |
| native | 1024 | 16 | 1024 | 0 | 3,269 | 4.69 | 6.74 | 606 | 0.0 |
| wasm | 1 | 1 | 256 | 0 | 357 | 2.79 | 2.89 | 164 | 0.0 |
| wasm | 16 | 16 | 256 | 0 | 2,446 | 5.80 | 8.36 | 163 | 0.0 |
| wasm | 64 | 16 | 256 | 0 | 2,556 | 6.05 | 7.11 | 238 | 0.0 |
| wasm | 256 | 16 | 256 | 0 | 2,357 | 6.73 | 7.13 | 480 | 0.0 |
| wasm | 1024 | 16 | 1024 | 0 | 2,369 | 6.73 | 7.04 | 1,507 | 0.0 |
| abi | 1 | 1 | 256 | 0 | 435 | 2.28 | 2.40 | 137 | 0.0 |
| abi | 16 | 16 | 256 | 0 | 3,278 | 4.67 | 6.04 | 162 | 0.0 |
| abi | 64 | 16 | 256 | 0 | 3,088 | 4.94 | 5.97 | 242 | 0.0 |
| abi | 256 | 16 | 256 | 0 | 2,758 | 5.71 | 6.13 | 495 | 0.0 |
| abi | 1024 | 16 | 1024 | 0 | 2,785 | 5.70 | 6.02 | 1,575 | 0.0 |
| abi-uu | 1 | 1 | 256 | 0 | 433 | 2.30 | 2.37 | 137 | 0.0 |
| abi-uu | 16 | 16 | 256 | 0 | 2,823 | 4.55 | 10.71 | 162 | 0.0 |
| abi-uu | 64 | 16 | 256 | 0 | 3,166 | 4.80 | 5.87 | 242 | 0.0 |
| abi-uu | 256 | 16 | 256 | 0 | 2,743 | 5.61 | 5.85 | 493 | 0.0 |
| abi-uu | 1024 | 16 | 1024 | 0 | 2,805 | 5.61 | 6.01 | 1,564 | 0.0 |
| child | 1 | 1 | 256 | 0 | 177 | 5.58 | 6.09 | 131 | 3.6 |
| child | 16 | 16 | 256 | 0 | 1,269 | 11.48 | 17.54 | 131 | 21.6 |
| child | 64 | 16 | 256 | 0 | 1,140 | 12.11 | 23.06 | 131 | 77.5 |
| child | 256 | 16 | 256 | 0 | 1,067 | 13.62 | 23.14 | 131 | 226 |
| child | 1024 | 16 | 1024 | 0 | 1,177 | 12.76 | 18.64 | 131 | 893 |

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
| runaway loop abi engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 378 ms after start |
| runaway loop abi fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.061 ms after the stop |
| runaway loop abi park at 200 ms (grace 100 ms) | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.008 ms after the stop |
| runaway loop abi call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 200 ms after start |
| runaway native abi engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native abi fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 3269.969 ms after the stop |
| runaway native abi park at 200 ms (grace 100 ms) | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 101.259 ms after the stop |
| runaway native abi call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| memory arena 64 MiB abi | survived | MemoryError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 512 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 512 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 4096 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| panic abi-uu release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| runaway loop abi-uu engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 383 ms after start |
| runaway loop abi-uu fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.047 ms after the stop |
| runaway loop abi-uu park at 200 ms (grace 100 ms) | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.015 ms after the stop |
| runaway loop abi-uu call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 200 ms after start |
| runaway native abi-uu engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native abi-uu fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 3198.045 ms after the stop |
| runaway native abi-uu park at 200 ms (grace 100 ms) | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 100.971 ms after the stop |
| runaway native abi-uu call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 200 ms after start |
| memory arena 64 MiB abi-uu | survived | MemoryError | completed: 2 | completed | ok |  |
| panic abi-uu shipped guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic native release (panic=abort) | **died: killed by SIGABRT** | test hook: injected panic in start; note: run with `RUST_BACKTRACE=1` environment variable | lost with the host | lost with the host | — | — |
| panic native release-unwind | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic native shipped (immediate-abort) | **died: killed by SIGILL** |  | lost with the host | lost with the host | — | — |
| panic wasm release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic wasm shipped guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic child release worker (panic=abort) | survived | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | completed | ok |  |
| panic child release-unwind worker | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic child shipped worker | survived | lost: worker exited: signal: 4 (SIGILL) (core dumped) | lost: worker exited: signal: 4 (SIGILL) (core dumped) | completed | ok |  |
| runaway loop native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 256 ms after start |
| runaway loop native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.054 ms after the stop |
| runaway loop native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.003 ms after the stop |
| runaway loop wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 334 ms after start |
| runaway loop wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.013 ms after the stop |
| runaway loop wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.041 ms after the stop |
| runaway loop child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 246 ms after start |
| runaway loop child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.039 ms after the stop |
| runaway loop child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.070 ms after the stop |
| runaway loop wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| runaway loop wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 115 ms after start |
| runaway loop child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.331 ms after the stop |
| runaway loop child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 1001 ms after start |
| runaway native native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 1475.705 ms after the stop |
| runaway native native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 3946.078 ms after the stop |
| runaway native wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 3248.256 ms after the stop |
| runaway native wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.044 ms after the stop |
| runaway native child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 1463.489 ms after the stop |
| runaway native child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 3881.185 ms after the stop |
| runaway native wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| runaway native wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 59 ms after start |
| runaway native child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.490 ms after the stop |
| runaway native child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 1000 ms after start |
| memory arena 64 MiB native | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB wasm | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB child | survived | MemoryError | completed: 2 | completed | ok |  |
| memory outside arena (completion value) native | survived | completed:  (host peak RSS 114 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) wasm memory_limit_mib=256 | survived | completed:  (host peak RSS 174 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) abi memory_limit_mib=256 | survived | completed:  (host peak RSS 176 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) abi-uu memory_limit_mib=256 | survived | completed:  (host peak RSS 176 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child | survived | completed:  (host peak RSS 6 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child prlimit=--as=268435456 | survived | completed:  (host peak RSS 6 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) native | survived | MemoryError (host peak RSS 113 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) wasm memory_limit_mib=256 | survived | MemoryError (host peak RSS 120 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) abi memory_limit_mib=256 | survived | MemoryError (host peak RSS 119 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) abi-uu memory_limit_mib=256 | survived | MemoryError (host peak RSS 119 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child | survived | MemoryError (host peak RSS 4 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child prlimit=--as=268435456 | survived | MemoryError (host peak RSS 4 MiB) | completed: 2 | completed | ok |  |

