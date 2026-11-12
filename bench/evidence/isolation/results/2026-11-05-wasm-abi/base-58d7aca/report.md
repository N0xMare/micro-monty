## mm-wasm guest size (shipped build) [M]

- wasm32-wasip1 mm_wasm_guest.wasm 2094603 bytes, 1909631 after wasm-opt -Oz
- wasm32-unknown-unknown mm_wasm_guest.wasm 2187068 bytes, 1996809 after wasm-opt -Oz

## Startup [M]

| cold: mode | main → first cell done (ms) | module load (ms) | spawn → exit (ms) |
|---|---|---|---|
| native | 0.24 | 0.00 | 1.93 |
| wasm | 1.49 | 0.39 | 3.84 |
| wasm-compile | 273 | 272 | 285 |
| abi | 1.13 | 0.36 | 3.13 |
| abi-uu | 1.24 | 0.35 | 3.36 |
| child | 2.06 | 0.00 | 4.08 |

| warm: mode | what | session ready (µs) | first cell done (µs, median) | p99 (µs) |
|---|---|---|---|---|
| native | new container + session | 43.3 | 46.8 | 84.6 |
| wasm | new container + session | 183 | 201 | 320 |
| wasm-pool | new container + session | 174 | 191 | 277 |
| abi | new container + session | 186 | 200 | 323 |
| abi-uu | new container + session | 186 | 200 | 326 |
| child | new container + session | 1,844 | 1,984 | 2,278 |
| native | new session in an existing container | — | 47.1 | 101 |
| wasm | new session in an existing container | — | 108 | 178 |
| child | new session in an existing container | — | 152 | 269 |

## Host calls [M]

| mode | host call round trip (µs) | loop with host call (µs/iter) | loop with Python call (µs/iter) | codemode cell, 400 calls (ms) | codemode µs per call |
|---|---|---|---|---|---|
| native | 0.18 | 0.23 | 0.05 | 1.98 | 4.94 |
| wasm | 2.46 | 2.57 | 0.10 | 5.57 | 13.92 |
| abi | 0.94 | 1.04 | 0.10 | 4.43 | 11.07 |
| abi-uu | 0.89 | 0.99 | 0.11 | 4.29 | 10.72 |
| child | 51.64 | 51.69 | 0.05 | 27.31 | 68.28 |
| abi-uu-opt | 0.91 | 1.02 | 0.10 | 4.30 | 10.76 |

## Bench programs, ms per run (median) [M]

| program | native | wasm | wasm-plain | wasm-fuel | abi | abi-uu | child | abi-uu-opt |
|---|---|---|---|---|---|---|---|---|
| startup | 0.01 | 0.03 | 0.02 | 0.03 | 0.02 | 0.03 | 0.16 | 0.03 |
| fib | 23.07 | 51.76 | 49.32 | 51.31 | 52.17 | 56.41 | 24.35 | 53.77 |
| loop | 69.90 | 144 | 122 | 160 | 143 | 151 | 67.68 | 145 |
| dict | 117 | 245 | 185 | 241 | 238 | 244 | 117 | 223 |
| str | 45.24 | 88.02 | 74.21 | 94.59 | 88.72 | 89.96 | 47.55 | 88.22 |
| json_rows | 108 | 235 | 199 | 251 | 234 | 240 | 108 | 237 |
| closure | 48.67 | 100 | 76.66 | 102 | 102 | 101 | 45.02 | 96.51 |
| classes | 140 | 307 | 255 | 351 | 329 | 307 | 152 | 278 |
| generators | 274 | 614 | 528 | 670 | 622 | 613 | 302 | 615 |
| exceptions | 96.46 | 211 | 181 | 225 | 213 | 214 | 98.57 | 216 |
| fmt | 216 | 458 | 383 | 498 | 459 | 447 | 234 | 426 |
| sort | 161 | 344 | 278 | 363 | 360 | 332 | 191 | 344 |
| interp | 257 | 573 | 483 | 646 | 583 | 569 | 265 | 560 |
| **geomean vs native** (excl. startup) | 1.00× | 2.14× | 1.79× | 2.28× | 2.17× | 2.16× | 1.04× | 2.10× |

## Snapshots [M]

| state | mode | bytes | snapshot (µs) | restore (µs) |
|---|---|---|---|---|
| codemode@call100 | native | 56,001 | 59.3 | 113 |
| dicts10k@call | native | 1,891,570 | 1,921 | 953 |
| dicts100k@call | native | 17,999,715 | 25,732 | 20,292 |
| codemode@call100 | wasm | 56,001 | 641 | 746 |
| dicts10k@call | wasm | 1,891,570 | 5,150 | 3,734 |
| dicts100k@call | wasm | 17,999,715 | 65,790 | 49,125 |
| codemode@call100 | abi | 56,001 | 596 | 737 |
| dicts10k@call | abi | 1,891,570 | 4,251 | 3,665 |
| dicts100k@call | abi | 17,999,715 | 63,940 | 48,654 |
| codemode@call100 | abi-uu | 56,001 | 554 | 716 |
| dicts10k@call | abi-uu | 1,891,570 | 3,931 | 3,490 |
| dicts100k@call | abi-uu | 17,999,715 | 63,259 | 49,065 |
| codemode@call100 | child | 56,001 | 637 | 2,258 |
| dicts10k@call | child | 1,891,570 | 5,760 | 6,369 |
| dicts100k@call | child | 17,999,715 | 75,787 | 49,171 |

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
| native | idle | 107 | — | — | — |
| native | active | 564 | — | — | — |
| wasm | idle | 454 | 1,664 | — | — |
| wasm | active | 1,559 | 2,560 | — | — |
| abi | idle | 422 | 1,664 | — | — |
| abi | active | 1,526 | 2,560 | — | — |
| abi-uu | idle | 415 | 1,792 | — | — |
| abi-uu | active | 1,453 | 2,688 | — | — |
| child | idle | 0.0 | — | 1,056 | 396 |
| child | active | 0.0 | — | 3,416 | 1,040 |

## Many sessions on one host (codemode cells) [M]

| mode | sessions | threads | cells | errors | cells/s | p50 (ms) | p99 (ms) | host RSS (MiB) | workers PSS (MiB) |
|---|---|---|---|---|---|---|---|---|---|
| native | 1 | 1 | 256 | 0 | 486 | 2.03 | 2.34 | 6.3 | 0.0 |
| native | 16 | 16 | 256 | 0 | 4,561 | 2.12 | 12.01 | 22.5 | 0.0 |
| native | 64 | 16 | 256 | 0 | 2,932 | 2.34 | 13.87 | 60.3 | 0.0 |
| native | 256 | 16 | 256 | 0 | 1,331 | 10.85 | 19.80 | 212 | 0.0 |
| native | 1024 | 16 | 1024 | 0 | 1,438 | 10.46 | 18.46 | 803 | 0.0 |
| wasm | 1 | 1 | 256 | 0 | 163 | 5.77 | 10.40 | 162 | 0.0 |
| wasm | 16 | 16 | 256 | 0 | 1,541 | 5.83 | 14.09 | 163 | 0.0 |
| wasm | 64 | 16 | 256 | 0 | 2,114 | 5.97 | 9.69 | 239 | 0.0 |
| wasm | 256 | 16 | 256 | 0 | 1,480 | 7.53 | 13.90 | 544 | 0.0 |
| wasm | 1024 | 16 | 1024 | 0 | 1,804 | 7.68 | 13.10 | 1,762 | 0.0 |
| abi | 1 | 1 | 256 | 0 | 229 | 4.35 | 4.76 | 138 | 0.0 |
| abi | 16 | 16 | 256 | 0 | 3,002 | 4.42 | 9.60 | 165 | 0.0 |
| abi | 64 | 16 | 256 | 0 | 2,520 | 4.52 | 10.97 | 245 | 0.0 |
| abi | 256 | 16 | 256 | 0 | 1,590 | 8.05 | 13.98 | 568 | 0.0 |
| abi | 1024 | 16 | 1024 | 0 | 1,614 | 8.70 | 22.06 | 1,858 | 0.0 |
| abi-uu | 1 | 1 | 256 | 0 | 217 | 4.35 | 6.71 | 138 | 0.0 |
| abi-uu | 16 | 16 | 256 | 0 | 1,852 | 4.52 | 12.40 | 165 | 0.0 |
| abi-uu | 64 | 16 | 256 | 0 | 2,544 | 4.42 | 9.10 | 245 | 0.0 |
| abi-uu | 256 | 16 | 256 | 0 | 1,619 | 6.93 | 11.49 | 566 | 0.0 |
| abi-uu | 1024 | 16 | 1024 | 0 | 1,917 | 7.00 | 25.16 | 1,850 | 0.0 |
| child | 1 | 1 | 256 | 0 | 37.7 | 25.60 | 31.47 | 133 | 3.6 |
| child | 16 | 16 | 256 | 0 | 739 | 23.35 | 43.22 | 133 | 21.9 |
| child | 64 | 16 | 256 | 0 | 438 | 35.47 | 43.88 | 133 | 77.6 |
| child | 256 | 16 | 256 | 0 | 437 | 35.96 | 40.30 | 133 | 295 |
| child | 1024 | 16 | 1024 | 0 | 438 | 35.96 | 45.04 | 133 | 1,169 |

## Fault matrix [M]

| scenario | host | victim | victim session after | bystander session | fresh session | time to control |
|---|---|---|---|---|---|---|
| stack-cyclic-dict native thread 256 KiB | **died: killed by SIGABRT** | thread '<unknown>' (259416) has overflowed its stack; fatal runtime error: stack overflow, | lost with the host | lost with the host | — | — |
| stack-cyclic-dict native thread 512 KiB | **died: killed by SIGABRT** | thread '<unknown>' (259418) has overflowed its stack; fatal runtime error: stack overflow, | lost with the host | lost with the host | — | — |
| stack-cyclic-dict native thread 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 2048 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 8192 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 512 KiB | survived | lost: trap: wasm trap: call stack exhausted | lost: instance dead: trap: wasm trap: call stack exhausted | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict child worker stack 256 KiB | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| stack-cyclic-dict child worker stack 512 KiB | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| stack-cyclic-dict child worker stack 16384 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 256 KiB | **died: killed by SIGABRT** | thread '<unknown>' (259468) has overflowed its stack; fatal runtime error: stack overflow, | lost with the host | lost with the host | — | — |
| stack-frozenset-600 native thread 512 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 2048 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 8192 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 wasm max_wasm_stack 512 KiB | survived | lost: trap: wasm trap: call stack exhausted | lost: instance dead: trap: wasm trap: call stack exhausted | completed | ok |  |
| stack-frozenset-600 wasm max_wasm_stack 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 wasm max_wasm_stack 4096 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 child worker stack 256 KiB | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| stack-frozenset-600 child worker stack 512 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 child worker stack 16384 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi max_wasm_stack 512 KiB | survived | lost: trap: wasm trap: call stack exhausted | lost: trap: wasm trap: call stack exhausted | completed | ok |  |
| stack-cyclic-dict abi max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi max_wasm_stack 512 KiB | survived | lost: trap: wasm trap: call stack exhausted | lost: trap: wasm trap: call stack exhausted | completed | ok |  |
| stack-frozenset-600 abi max_wasm_stack 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi max_wasm_stack 4096 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| panic abi release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| runaway loop abi engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 586 ms after start |
| runaway loop abi fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.083 ms after the stop |
| runaway loop abi park at 200 ms (grace 100 ms) | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.033 ms after the stop |
| runaway loop abi call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 209 ms after start |
| runaway native abi engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 16087 ms after start |
| runaway native abi fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 15846.460 ms after the stop |
| runaway native abi park at 200 ms (grace 100 ms) | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 110.120 ms after the stop |
| runaway native abi call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| memory arena 64 MiB abi | survived | MemoryError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 512 KiB | survived | lost: trap: wasm trap: call stack exhausted | lost: trap: wasm trap: call stack exhausted | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict abi-uu max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 512 KiB | survived | lost: trap: wasm trap: call stack exhausted | lost: trap: wasm trap: call stack exhausted | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 abi-uu max_wasm_stack 4096 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| panic abi-uu release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| runaway loop abi-uu engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 624 ms after start |
| runaway loop abi-uu fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.108 ms after the stop |
| runaway loop abi-uu park at 200 ms (grace 100 ms) | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.029 ms after the stop |
| runaway loop abi-uu call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| runaway native abi-uu engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 15683 ms after start |
| runaway native abi-uu fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 15462.230 ms after the stop |
| runaway native abi-uu park at 200 ms (grace 100 ms) | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 100.731 ms after the stop |
| runaway native abi-uu call budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
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
| runaway loop native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 337 ms after start |
| runaway loop native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.054 ms after the stop |
| runaway loop native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.015 ms after the stop |
| runaway loop wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 584 ms after start |
| runaway loop wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.058 ms after the stop |
| runaway loop wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.150 ms after the stop |
| runaway loop child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 381 ms after start |
| runaway loop child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.026 ms after the stop |
| runaway loop child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.174 ms after the stop |
| runaway loop wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 209 ms after start |
| runaway loop wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 235 ms after start |
| runaway loop child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.439 ms after the stop |
| runaway loop child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 999 ms after start |
| runaway native native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7403 ms after start |
| runaway native native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7063.523 ms after the stop |
| runaway native native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7276.914 ms after the stop |
| runaway native wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 16382 ms after start |
| runaway native wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 16172.805 ms after the stop |
| runaway native wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.105 ms after the stop |
| runaway native child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7512 ms after start |
| runaway native child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7144.292 ms after the stop |
| runaway native child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7189.030 ms after the stop |
| runaway native wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 208 ms after start |
| runaway native wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 113 ms after start |
| runaway native child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.506 ms after the stop |
| runaway native child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 999 ms after start |
| memory arena 64 MiB native | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB wasm | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB child | survived | MemoryError | completed: 2 | completed | ok |  |
| memory outside arena (completion value) native | survived | completed:  (host peak RSS 487 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) wasm memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 171 MiB) | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (completion value) abi memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 171 MiB) | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (completion value) abi-uu memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 170 MiB) | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (completion value) child | survived | completed:  (host peak RSS 4 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child prlimit=--as=268435456 | survived | lost: worker exited: signal: 6 (SIGABRT) (host peak RSS 2 MiB) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| memory outside arena (host-call argument) native | survived | HostCall { call: 1, name: "tool", args: [Array [Number(0), N (host peak RSS 645 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) wasm memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 118 MiB) | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (host-call argument) abi memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 117 MiB) | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (host-call argument) abi-uu memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 117 MiB) | lost: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (host-call argument) child | survived | HostCall { call: 1, name: "tool", args: [Array [Number(0), N (host peak RSS 443 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child prlimit=--as=268435456 | survived | lost: worker exited: signal: 6 (SIGABRT) (host peak RSS 2 MiB) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |

