## Startup [M]

| cold: mode | main → first cell done (ms) | module load (ms) | spawn → exit (ms) |
|---|---|---|---|
| native | 0.17 | 0.00 | 1.11 |
| wasm | 0.80 | 0.31 | 1.92 |
| wasm-compile | 177 | 177 | 188 |
| child | 1.02 | 0.00 | 2.04 |

| warm: mode | what | session ready (µs) | first cell done (µs, median) | p99 (µs) |
|---|---|---|---|---|
| native | new container + session | 22.1 | 23.9 | 40.6 |
| wasm | new container + session | 107 | 116 | 140 |
| wasm-pool | new container + session | 101 | 110 | 125 |
| child | new container + session | 832 | 868 | 1,144 |
| native | new session in an existing container | — | 24.2 | 41.5 |
| wasm | new session in an existing container | — | 59.6 | 73.6 |
| child | new session in an existing container | — | 45.9 | 75.9 |

## Host calls [M]

| mode | host call round trip (µs) | loop with host call (µs/iter) | loop with Python call (µs/iter) | codemode cell, 400 calls (ms) | codemode µs per call |
|---|---|---|---|---|---|
| native | 0.13 | 0.16 | 0.03 | 1.06 | 2.65 |
| wasm | 1.43 | 1.49 | 0.06 | 2.80 | 7.01 |
| child | 10.45 | 10.47 | 0.03 | 5.66 | 14.14 |

## Bench programs, ms per run (median) [M]

| program | native | wasm | wasm-plain | wasm-fuel | child |
|---|---|---|---|---|---|
| startup | 0.00 | 0.01 | 0.02 | 0.02 | 0.08 |
| fib | 14.45 | 28.68 | 23.68 | 27.73 | 14.16 |
| loop | 45.49 | 93.04 | 69.11 | 86.09 | 41.89 |
| dict | 62.76 | 119 | 103 | 120 | 65.33 |
| str | 25.95 | 48.15 | 42.09 | 50.67 | 28.48 |
| json_rows | 61.07 | 121 | 107 | 134 | 62.88 |
| closure | 28.98 | 60.19 | 45.24 | 57.25 | 28.88 |
| classes | 72.62 | 169 | 142 | 192 | 74.01 |
| generators | 153 | 323 | 265 | 338 | 153 |
| exceptions | 51.32 | 108 | 95.58 | 119 | 51.83 |
| fmt | 117 | 226 | 190 | 260 | 119 |
| sort | 81.52 | 161 | 139 | 176 | 80.54 |
| interp | 141 | 303 | 252 | 339 | 137 |
| **geomean vs native** (excl. startup) | 1.00× | 2.03× | 1.70× | 2.14× | 1.01× |

## Snapshots [M]

| state | mode | bytes | snapshot (µs) | restore (µs) |
|---|---|---|---|---|
| codemode@call100 | native | 56,001 | 34.0 | 62.5 |
| dicts10k@call | native | 1,891,570 | 1,267 | 561 |
| dicts100k@call | native | 17,999,715 | 14,554 | 11,538 |
| codemode@call100 | wasm | 56,001 | 442 | 526 |
| dicts10k@call | wasm | 1,891,570 | 3,021 | 2,522 |
| dicts100k@call | wasm | 17,999,715 | 43,024 | 29,537 |
| codemode@call100 | child | 56,001 | 327 | 1,298 |
| dicts10k@call | child | 1,891,570 | 3,229 | 3,852 |
| dicts100k@call | child | 17,999,715 | 43,334 | 25,008 |

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

| mode | state | host RSS Δ (KiB/session) | wasm linear memory (KiB) | worker RSS (KiB) | worker PSS (KiB) |
|---|---|---|---|---|---|
| native | idle | 69.8 | — | — | — |
| native | active | 385 | — | — | — |
| wasm | idle | 393 | 1,664 | — | — |
| wasm | active | 1,333 | 2,560 | — | — |
| child | idle | 1.3 | — | 4,036 | 336 |
| child | active | 0.0 | — | 5,112 | 809 |

## Many sessions on one host (codemode cells) [M]

| mode | sessions | threads | cells | errors | cells/s | p50 (ms) | p99 (ms) | host RSS (MiB) | workers PSS (MiB) |
|---|---|---|---|---|---|---|---|---|---|
| native | 1 | 1 | 256 | 0 | 917 | 1.08 | 1.20 | 7.5 | 0.0 |
| native | 16 | 16 | 256 | 0 | 6,537 | 2.18 | 5.44 | 23.3 | 0.0 |
| native | 64 | 16 | 256 | 0 | 5,366 | 2.30 | 5.47 | 62.0 | 0.0 |
| native | 256 | 16 | 256 | 0 | 3,204 | 4.83 | 5.98 | 156 | 0.0 |
| native | 1024 | 16 | 1024 | 0 | 3,273 | 4.84 | 6.21 | 596 | 0.0 |
| wasm | 1 | 1 | 256 | 0 | 357 | 2.79 | 2.85 | 166 | 0.0 |
| wasm | 16 | 16 | 256 | 0 | 2,652 | 5.77 | 8.14 | 165 | 0.0 |
| wasm | 64 | 16 | 256 | 0 | 2,551 | 6.06 | 7.05 | 240 | 0.0 |
| wasm | 256 | 16 | 256 | 0 | 2,337 | 6.77 | 7.16 | 484 | 0.0 |
| wasm | 1024 | 16 | 1024 | 0 | 2,347 | 6.78 | 7.16 | 1,521 | 0.0 |
| child | 1 | 1 | 256 | 0 | 176 | 5.63 | 6.20 | 133 | 3.5 |
| child | 16 | 16 | 256 | 0 | 1,297 | 11.52 | 17.39 | 131 | 21.6 |
| child | 64 | 16 | 256 | 0 | 1,177 | 11.91 | 23.37 | 131 | 77.4 |
| child | 256 | 16 | 256 | 0 | 1,081 | 13.82 | 19.91 | 131 | 226 |
| child | 1024 | 16 | 1024 | 0 | 1,177 | 12.78 | 18.51 | 131 | 892 |

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
| panic native shipped (immediate-abort) | **died: killed by SIGILL** |  | lost with the host | lost with the host | — | — |
| panic wasm release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic wasm shipped guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic child release worker (panic=abort) | survived | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | completed | ok |  |
| panic child release-unwind worker | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic child shipped worker | survived | lost: worker exited: signal: 4 (SIGILL) (core dumped) | lost: worker exited: signal: 4 (SIGILL) (core dumped) | completed | ok |  |
| runaway loop native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 248 ms after start |
| runaway loop native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.043 ms after the stop |
| runaway loop native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.002 ms after the stop |
| runaway loop wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 348 ms after start |
| runaway loop wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.031 ms after the stop |
| runaway loop wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.031 ms after the stop |
| runaway loop child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 236 ms after start |
| runaway loop child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.052 ms after the stop |
| runaway loop child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.200 ms after the stop |
| runaway loop wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| runaway loop wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 114 ms after start |
| runaway loop child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.228 ms after the stop |
| runaway loop child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 998 ms after start |
| runaway native native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 1498.097 ms after the stop |
| runaway native native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 3963.076 ms after the stop |
| runaway native wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 3298.980 ms after the stop |
| runaway native wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.034 ms after the stop |
| runaway native child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0 ms after start |
| runaway native child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 1474.115 ms after the stop |
| runaway native child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 3932.974 ms after the stop |
| runaway native wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| runaway native wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 58 ms after start |
| runaway native child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.517 ms after the stop |
| runaway native child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 1000 ms after start |
| memory arena 64 MiB native | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB wasm | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB child | survived | MemoryError | completed: 2 | completed | ok |  |
| memory outside arena (completion value) native | survived | completed:  (host peak RSS 115 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) wasm memory_limit_mib=256 | survived | completed:  (host peak RSS 173 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child | survived | completed:  (host peak RSS 7 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child prlimit=--as=268435456 | survived | completed:  (host peak RSS 6 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) native | survived | MemoryError (host peak RSS 113 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) wasm memory_limit_mib=256 | survived | MemoryError (host peak RSS 120 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child | survived | MemoryError (host peak RSS 5 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child prlimit=--as=268435456 | survived | MemoryError (host peak RSS 5 MiB) | completed: 2 | completed | ok |  |

