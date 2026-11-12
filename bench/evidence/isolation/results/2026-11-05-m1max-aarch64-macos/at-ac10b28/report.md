## Startup [M]

| cold: mode | main → first cell done (ms) | module load (ms) | spawn → exit (ms) |
|---|---|---|---|
| native | 0.15 | 0.00 | 3.13 |
| wasm | 1.70 | 1.06 | 5.14 |
| wasm-compile | 231 | 230 | 236 |
| child | 2.12 | 0.00 | 5.61 |

| warm: mode | what | session ready (µs) | first cell done (µs, median) | p99 (µs) |
|---|---|---|---|---|
| native | new container + session | 29.6 | 32.0 | 65.4 |
| wasm | new container + session | 96.2 | 101 | 181 |
| wasm-pool | new container + session | 93.8 | 99.0 | 131 |
| child | new container + session | 2,219 | 2,270 | 5,030 |
| native | new session in an existing container | — | 32.8 | 37.0 |
| wasm | new session in an existing container | — | 69.2 | 92.5 |
| child | new session in an existing container | — | 51.2 | 97.1 |

## Host calls [M]

| mode | host call round trip (µs) | loop with host call (µs/iter) | loop with Python call (µs/iter) | codemode cell, 400 calls (ms) | codemode µs per call |
|---|---|---|---|---|---|
| native | 0.14 | 0.17 | 0.03 | 1.21 | 3.02 |
| wasm | 1.38 | 1.45 | 0.07 | 3.05 | 7.62 |
| child | 9.64 | 9.68 | 0.03 | 5.88 | 14.69 |

## Bench programs, ms per run (median) [M]

| program | native | wasm | wasm-plain | wasm-fuel | child |
|---|---|---|---|---|---|
| startup | 0.01 | 0.01 | 0.01 | 0.01 | 0.08 |
| fib | 20.34 | 37.27 | 30.94 | 40.73 | 20.11 |
| loop | 46.31 | 93.12 | 79.29 | 95.24 | 46.23 |
| dict | 62.98 | 124 | 109 | 136 | 64.96 |
| str | 24.53 | 47.29 | 41.59 | 50.94 | 26.03 |
| json_rows | 58.68 | 126 | 107 | 137 | 59.17 |
| closure | 32.03 | 59.66 | 48.36 | 63.12 | 33.10 |
| classes | 87.18 | 159 | 134 | 179 | 86.36 |
| generators | 165 | 368 | 311 | 383 | 163 |
| exceptions | 53.48 | 109 | 93.49 | 115 | 53.98 |
| fmt | 116 | 221 | 170 | 230 | 118 |
| sort | 86.32 | 178 | 142 | 185 | 84.69 |
| interp | 159 | 330 | 270 | 348 | 159 |
| **geomean vs native** (excl. startup) | 1.00× | 1.99× | 1.66× | 2.12× | 1.01× |

## Snapshots [M]

| state | mode | bytes | snapshot (µs) | restore (µs) |
|---|---|---|---|---|
| codemode@call100 | native | 56,001 | 39.7 | 74.6 |
| dicts10k@call | native | 1,891,570 | 984 | 733 |
| dicts100k@call | native | 17,999,715 | 11,195 | 6,509 |
| codemode@call100 | wasm | 56,001 | 296 | 353 |
| dicts10k@call | wasm | 1,891,570 | 2,328 | 1,759 |
| dicts100k@call | wasm | 17,999,715 | 21,500 | 14,155 |
| codemode@call100 | child | 56,001 | 308 | 2,285 |
| dicts10k@call | child | 1,891,570 | 2,002 | 3,728 |
| dicts100k@call | child | 17,999,715 | 19,294 | 15,208 |

| saved in | restored in | result |
|---|---|---|
| native | native | resumed, output identical |
| native | wasm | resumed, output identical |
| native | child | resumed, output identical |
| wasm | native | resumed, output identical |
| wasm | wasm | resumed, output identical |
| wasm | child | resumed, output identical |
| child | native | resumed, output identical |
| child | wasm | resumed, output identical |
| child | child | resumed, output identical |

## Memory per session (100 sessions) [M]

| mode | state | host RSS Δ (KiB/session) | wasm linear memory (KiB) | worker RSS (KiB) | worker phys_footprint (KiB) |
|---|---|---|---|---|---|
| native | idle | 86.4 | — | — | — |
| native | active | 396 | — | — | — |
| wasm | idle | 368 | 1,664 | — | — |
| wasm | active | 1,218 | 2,560 | — | — |
| child | idle | 0.5 | — | 2,848 | 1,456 |
| child | active | 0.3 | — | 4,544 | 2,672 |

## Many sessions on one host (codemode cells) [M]

| mode | sessions | threads | cells | errors | cells/s | p50 (ms) | p99 (ms) | host RSS (MiB) | workers phys_footprint (MiB) |
|---|---|---|---|---|---|---|---|---|---|
| native | 1 | 1 | 256 | 0 | 865 | 1.13 | 1.45 | 6.2 | 0.0 |
| native | 16 | 10 | 256 | 0 | 3,900 | 1.52 | 6.25 | 21.5 | 0.0 |
| native | 64 | 10 | 256 | 0 | 4,297 | 1.55 | 10.08 | 59.1 | 0.0 |
| native | 256 | 10 | 256 | 0 | 3,919 | 1.77 | 5.74 | 153 | 0.0 |
| native | 1024 | 10 | 1024 | 0 | 4,597 | 1.59 | 7.72 | 558 | 0.0 |
| wasm | 1 | 1 | 256 | 0 | 346 | 2.86 | 3.57 | 564 | 0.0 |
| wasm | 16 | 10 | 256 | 0 | 1,895 | 3.14 | 13.06 | 587 | 0.0 |
| wasm | 64 | 10 | 256 | 0 | 1,927 | 3.24 | 14.65 | 659 | 0.0 |
| wasm | 256 | 10 | 256 | 0 | 1,871 | 3.56 | 13.41 | 891 | 0.0 |
| wasm | 1024 | 10 | 1024 | 0 | 2,098 | 3.43 | 14.30 | 1,875 | 0.0 |
| child | 1 | 1 | 256 | 0 | 164 | 5.68 | 11.01 | 592 | 3.9 |
| child | 16 | 10 | 256 | 0 | 306 | 27.68 | 83.25 | 592 | 54.2 |
| child | 64 | 10 | 256 | 0 | 281 | 34.23 | 65.49 | 592 | 205 |
| child | 256 | 10 | 256 | 0 | 289 | 34.54 | 49.54 | 594 | 700 |
| child | 1024 | 10 | 1024 | 0 | 282 | 34.30 | 67.37 | 89.1 | 2,806 |

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
| panic native release (panic=abort) | **died: killed by SIGABRT** | test hook: injected panic in start; note: run with `RUST_BACKTRACE=1` environment variable | lost with the host | lost with the host | — | — |
| panic native release-unwind | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic native shipped (immediate-abort) | **died: killed by SIGTRAP** |  | lost with the host | lost with the host | — | — |
| panic wasm release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic wasm shipped guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic child release worker (panic=abort) | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| panic child release-unwind worker | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic child shipped worker | survived | lost: worker exited: signal: 5 (SIGTRAP) | lost: worker exited: signal: 5 (SIGTRAP) | completed | ok |  |
| runaway loop native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 328 ms after start |
| runaway loop native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.017 ms after the stop |
| runaway loop native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.006 ms after the stop |
| runaway loop wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 580 ms after start |
| runaway loop wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.049 ms after the stop |
| runaway loop wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.458 ms after the stop |
| runaway loop child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 331 ms after start |
| runaway loop child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.019 ms after the stop |
| runaway loop child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.059 ms after the stop |
| runaway loop wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 249 ms after start |
| runaway loop wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 254 ms after start |
| runaway loop child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.513 ms after the stop |
| runaway loop child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 24 (SIGXCPU) | lost: worker exited: signal: 24 (SIGXCPU) | completed | ok | 1002 ms after start |
| runaway native native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 2819.238 ms after the stop |
| runaway native native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7270.557 ms after the stop |
| runaway native wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 4771.747 ms after the stop |
| runaway native wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.663 ms after the stop |
| runaway native child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 2822.647 ms after the stop |
| runaway native child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7286.045 ms after the stop |
| runaway native wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 237 ms after start |
| runaway native wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 69 ms after start |
| runaway native child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.602 ms after the stop |
| runaway native child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 24 (SIGXCPU) | lost: worker exited: signal: 24 (SIGXCPU) | completed | ok | 1002 ms after start |
| memory arena 64 MiB native | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB wasm | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB child | survived | MemoryError | completed: 2 | completed | ok |  |
| memory outside arena (completion value) native | survived | completed:  (host peak RSS 113 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) wasm memory_limit_mib=256 | survived | completed:  (host peak RSS 170 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child | survived | completed:  (host peak RSS 5 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child prlimit=--as=268435456 | survived | lost: not started: spawn /Users/local/dev/agents/micro-monty/target/isolation/release/iso-worker: Invalid argument (os error 22) | — | — | — |  |
| memory outside arena (host-call argument) native | survived | MemoryError (host peak RSS 111 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) wasm memory_limit_mib=256 | survived | MemoryError (host peak RSS 116 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child | survived | MemoryError (host peak RSS 3 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child prlimit=--as=268435456 | survived | lost: not started: spawn /Users/local/dev/agents/micro-monty/target/isolation/release/iso-worker: Invalid argument (os error 22) | — | — | — |  |

