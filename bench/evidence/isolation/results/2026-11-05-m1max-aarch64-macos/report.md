## mm-wasm guest size (shipped build) [M]

- wasm32-wasip1 mm_wasm_guest.wasm 2111851 bytes, 1926729 after wasm-opt -Oz
- wasm32-unknown-unknown mm_wasm_guest.wasm 2205342 bytes, 2015061 after wasm-opt -Oz

## Startup [M]

| cold: mode | main → first cell done (ms) | module load (ms) | spawn → exit (ms) |
|---|---|---|---|
| native | 0.15 | 0.00 | 2.94 |
| wasm | 1.80 | 1.08 | 5.15 |
| wasm-compile | 221 | 220 | 226 |
| abi | 1.74 | 1.12 | 5.02 |
| abi-uu | 1.79 | 1.13 | 5.15 |
| child | 2.42 | 0.00 | 6.10 |

| warm: mode | what | session ready (µs) | first cell done (µs, median) | p99 (µs) |
|---|---|---|---|---|
| native | new container + session | 29.6 | 32.0 | 52.0 |
| wasm | new container + session | 96.2 | 101 | 148 |
| wasm-pool | new container + session | 91.0 | 95.9 | 128 |
| abi | new container + session | 94.4 | 99.0 | 1,067 |
| abi-uu | new container + session | 95.5 | 101 | 163 |
| child | new container + session | 2,208 | 2,250 | 3,693 |
| native | new session in an existing container | — | 33.0 | 50.0 |
| wasm | new session in an existing container | — | 68.1 | 93.0 |
| child | new session in an existing container | — | 54.8 | 117 |

## Host calls [M]

| mode | host call round trip (µs) | loop with host call (µs/iter) | loop with Python call (µs/iter) | codemode cell, 400 calls (ms) | codemode µs per call |
|---|---|---|---|---|---|
| native | 0.14 | 0.17 | 0.03 | 1.17 | 2.94 |
| wasm | 1.37 | 1.43 | 0.06 | 2.88 | 7.20 |
| abi | 0.60 | 0.67 | 0.06 | 2.42 | 6.04 |
| abi-uu | 0.58 | 0.65 | 0.07 | 2.37 | 5.92 |
| child | 9.23 | 9.26 | 0.03 | 5.89 | 14.74 |
| abi-uu-opt | 0.61 | 0.67 | 0.06 | 2.48 | 6.20 |

## Bench programs, ms per run (median) [M]

| program | native | wasm | wasm-plain | wasm-fuel | abi | abi-uu | child | abi-uu-opt |
|---|---|---|---|---|---|---|---|---|
| startup | 0.01 | 0.01 | 0.01 | 0.01 | 0.01 | 0.01 | 0.08 | 0.01 |
| fib | 20.66 | 37.41 | 30.76 | 41.66 | 37.03 | 37.97 | 20.02 | 36.11 |
| loop | 46.75 | 92.93 | 77.54 | 97.02 | 92.92 | 90.01 | 46.81 | 88.54 |
| dict | 63.47 | 123 | 105 | 133 | 124 | 126 | 65.67 | 126 |
| str | 24.87 | 47.25 | 41.46 | 51.29 | 47.56 | 47.98 | 25.98 | 50.35 |
| json_rows | 58.67 | 127 | 106 | 138 | 127 | 128 | 61.34 | 127 |
| closure | 31.83 | 60.39 | 48.58 | 63.84 | 61.16 | 61.08 | 33.57 | 58.18 |
| classes | 86.70 | 159 | 133 | 174 | 161 | 159 | 88.24 | 151 |
| generators | 168 | 364 | 310 | 387 | 368 | 371 | 164 | 350 |
| exceptions | 53.83 | 108 | 93.47 | 114 | 110 | 109 | 54.22 | 105 |
| fmt | 114 | 211 | 176 | 228 | 215 | 212 | 116 | 208 |
| sort | 88.36 | 168 | 161 | 182 | 174 | 168 | 92.42 | 169 |
| interp | 159 | 324 | 278 | 352 | 329 | 321 | 163 | 310 |
| **geomean vs native** (excl. startup) | 1.00× | 1.95× | 1.67× | 2.11× | 1.98× | 1.96× | 1.02× | 1.92× |

## Snapshots [M]

| state | mode | bytes | snapshot (µs) | restore (µs) |
|---|---|---|---|---|
| codemode@call100 | native | 56,001 | 45.0 | 82.5 |
| dicts10k@call | native | 1,891,570 | 1,047 | 759 |
| dicts100k@call | native | 17,999,715 | 10,878 | 6,608 |
| codemode@call100 | wasm | 56,001 | 324 | 389 |
| dicts10k@call | wasm | 1,891,570 | 2,251 | 1,747 |
| dicts100k@call | wasm | 17,999,715 | 22,440 | 14,315 |
| codemode@call100 | abi | 56,001 | 276 | 384 |
| dicts10k@call | abi | 1,891,570 | 2,009 | 1,754 |
| dicts100k@call | abi | 17,999,715 | 21,279 | 14,671 |
| codemode@call100 | abi-uu | 56,001 | 274 | 362 |
| dicts10k@call | abi-uu | 1,891,570 | 2,129 | 1,758 |
| dicts100k@call | abi-uu | 17,999,715 | 20,961 | 14,350 |
| codemode@call100 | child | 56,001 | 326 | 2,304 |
| dicts10k@call | child | 1,891,570 | 2,048 | 3,798 |
| dicts100k@call | child | 17,999,715 | 19,863 | 15,031 |

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

| mode | state | host RSS Δ (KiB/session) | wasm linear memory (KiB) | worker RSS (KiB) | worker phys_footprint (KiB) |
|---|---|---|---|---|---|
| native | idle | 86.4 | — | — | — |
| native | active | 384 | — | — | — |
| wasm | idle | 368 | 1,664 | — | — |
| wasm | active | 1,217 | 2,560 | — | — |
| abi | idle | 228 | 1,600 | — | — |
| abi | active | 1,168 | 2,560 | — | — |
| abi-uu | idle | 272 | 1,728 | — | — |
| abi-uu | active | 1,184 | 2,816 | — | — |
| child | idle | 8.2 | — | 2,832 | 1,440 |
| child | active | 1.0 | — | 4,592 | 2,704 |

## Many sessions on one host (codemode cells) [M]

| mode | sessions | threads | cells | errors | cells/s | p50 (ms) | p99 (ms) | host RSS (MiB) | workers phys_footprint (MiB) |
|---|---|---|---|---|---|---|---|---|---|
| native | 1 | 1 | 256 | 0 | 870 | 1.13 | 1.31 | 6.2 | 0.0 |
| native | 16 | 10 | 256 | 0 | 3,410 | 1.63 | 9.86 | 21.3 | 0.0 |
| native | 64 | 10 | 256 | 0 | 3,900 | 1.51 | 7.06 | 56.7 | 0.0 |
| native | 256 | 10 | 256 | 0 | 3,226 | 1.70 | 11.22 | 146 | 0.0 |
| native | 1024 | 10 | 1024 | 0 | 4,170 | 1.58 | 12.08 | 543 | 0.0 |
| wasm | 1 | 1 | 256 | 0 | 328 | 2.96 | 3.96 | 550 | 0.0 |
| wasm | 16 | 10 | 256 | 0 | 1,924 | 3.21 | 11.27 | 572 | 0.0 |
| wasm | 64 | 10 | 256 | 0 | 1,919 | 3.44 | 14.57 | 644 | 0.0 |
| wasm | 256 | 10 | 256 | 0 | 1,677 | 3.71 | 14.70 | 876 | 0.0 |
| wasm | 1024 | 10 | 1024 | 0 | 1,892 | 3.47 | 19.26 | 1,860 | 0.0 |
| abi | 1 | 1 | 256 | 0 | 410 | 2.42 | 2.67 | 582 | 0.0 |
| abi | 16 | 10 | 256 | 0 | 2,047 | 2.68 | 12.29 | 605 | 0.0 |
| abi | 64 | 10 | 256 | 0 | 2,132 | 2.79 | 12.31 | 679 | 0.0 |
| abi | 256 | 10 | 256 | 0 | 1,940 | 3.27 | 12.36 | 917 | 0.0 |
| abi | 1024 | 10 | 1024 | 0 | 2,190 | 2.98 | 15.64 | 1,929 | 0.0 |
| abi-uu | 1 | 1 | 256 | 0 | 414 | 2.41 | 2.59 | 587 | 0.0 |
| abi-uu | 16 | 10 | 256 | 0 | 2,281 | 2.67 | 12.19 | 610 | 0.0 |
| abi-uu | 64 | 10 | 256 | 0 | 2,323 | 2.72 | 11.49 | 686 | 0.0 |
| abi-uu | 256 | 10 | 256 | 0 | 2,184 | 3.17 | 11.23 | 925 | 0.0 |
| abi-uu | 1024 | 10 | 1024 | 0 | 2,259 | 3.01 | 16.87 | 1,945 | 0.0 |
| child | 1 | 1 | 256 | 0 | 170 | 5.87 | 6.88 | 582 | 3.9 |
| child | 16 | 10 | 256 | 0 | 333 | 21.85 | 66.18 | 582 | 54.8 |
| child | 64 | 10 | 256 | 0 | 289 | 30.90 | 92.24 | 582 | 203 |
| child | 256 | 10 | 256 | 0 | 277 | 34.96 | 67.87 | 584 | 710 |
| child | 1024 | 10 | 1024 | 0 | 272 | 36.11 | 68.23 | 245 | 2,848 |

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
| runaway loop abi engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 495 ms after start |
| runaway loop abi fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.110 ms after the stop |
| runaway loop abi park at 200 ms (grace 100 ms) | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.018 ms after the stop |
| runaway loop abi call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 208 ms after start |
| runaway native abi engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native abi fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 4925.720 ms after the stop |
| runaway native abi park at 200 ms (grace 100 ms) | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 111.940 ms after the stop |
| runaway native abi call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 203 ms after start |
| memory arena 64 MiB abi | survived | MemoryError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 512 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 512 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 4096 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| panic abi-uu release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| runaway loop abi-uu engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 569 ms after start |
| runaway loop abi-uu fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.038 ms after the stop |
| runaway loop abi-uu park at 200 ms (grace 100 ms) | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.021 ms after the stop |
| runaway loop abi-uu call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 202 ms after start |
| runaway native abi-uu engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native abi-uu fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 4773.444 ms after the stop |
| runaway native abi-uu park at 200 ms (grace 100 ms) | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 101.606 ms after the stop |
| runaway native abi-uu call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 207 ms after start |
| memory arena 64 MiB abi-uu | survived | MemoryError | completed: 2 | completed | ok |  |
| panic abi-uu shipped guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic native release (panic=abort) | **died: killed by SIGABRT** | test hook: injected panic in start; note: run with `RUST_BACKTRACE=1` environment variable | lost with the host | lost with the host | — | — |
| panic native release-unwind | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic native shipped (immediate-abort) | **died: killed by SIGTRAP** |  | lost with the host | lost with the host | — | — |
| panic wasm release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic wasm shipped guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic child release worker (panic=abort) | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| panic child release-unwind worker | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic child shipped worker | survived | lost: worker exited: signal: 5 (SIGTRAP) | lost: worker exited: signal: 5 (SIGTRAP) | completed | ok |  |
| runaway loop native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 325 ms after start |
| runaway loop native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.053 ms after the stop |
| runaway loop native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.007 ms after the stop |
| runaway loop wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 486 ms after start |
| runaway loop wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.033 ms after the stop |
| runaway loop wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.459 ms after the stop |
| runaway loop child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 322 ms after start |
| runaway loop child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.094 ms after the stop |
| runaway loop child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.046 ms after the stop |
| runaway loop wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 278 ms after start |
| runaway loop wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 246 ms after start |
| runaway loop child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.549 ms after the stop |
| runaway loop child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 24 (SIGXCPU) | lost: worker exited: signal: 24 (SIGXCPU) | completed | ok | 1002 ms after start |
| runaway native native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 2848.087 ms after the stop |
| runaway native native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7310.306 ms after the stop |
| runaway native wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 4786.520 ms after the stop |
| runaway native wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.396 ms after the stop |
| runaway native child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 2870.059 ms after the stop |
| runaway native child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7344.700 ms after the stop |
| runaway native wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 277 ms after start |
| runaway native wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 68 ms after start |
| runaway native child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.552 ms after the stop |
| runaway native child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 24 (SIGXCPU) | lost: worker exited: signal: 24 (SIGXCPU) | completed | ok | 1013 ms after start |
| memory arena 64 MiB native | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB wasm | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB child | survived | MemoryError | completed: 2 | completed | ok |  |
| memory outside arena (completion value) native | survived | completed:  (host peak RSS 113 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) wasm memory_limit_mib=256 | survived | completed:  (host peak RSS 170 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) abi memory_limit_mib=256 | survived | completed:  (host peak RSS 173 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) abi-uu memory_limit_mib=256 | survived | completed:  (host peak RSS 173 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child | survived | completed:  (host peak RSS 5 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child prlimit=--as=268435456 | survived | lost: not started: spawn /Users/local/dev/agents/micro-monty/target/isolation/release/iso-worker: Invalid argument (os error 22) | — | — | — |  |
| memory outside arena (host-call argument) native | survived | MemoryError (host peak RSS 111 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) wasm memory_limit_mib=256 | survived | MemoryError (host peak RSS 116 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) abi memory_limit_mib=256 | survived | MemoryError (host peak RSS 115 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) abi-uu memory_limit_mib=256 | survived | MemoryError (host peak RSS 116 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child | survived | MemoryError (host peak RSS 3 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child prlimit=--as=268435456 | survived | lost: not started: spawn /Users/local/dev/agents/micro-monty/target/isolation/release/iso-worker: Invalid argument (os error 22) | — | — | — |  |

