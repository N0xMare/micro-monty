## Startup [M]

| cold: mode | main → first cell done (ms) | module load (ms) | spawn → exit (ms) |
|---|---|---|---|
| native | 0.25 | 0.00 | 1.31 |
| wasm | 0.87 | 0.35 | 2.17 |
| wasm-compile | 174 | 173 | 184 |
| child | 1.04 | 0.00 | 2.14 |

| warm: mode | what | session ready (µs) | first cell done (µs, median) | p99 (µs) |
|---|---|---|---|---|
| native | new container + session | 23.2 | 25.1 | 55.3 |
| wasm | new container + session | 126 | 134 | 156 |
| wasm-pool | new container + session | 122 | 130 | 146 |
| child | new container + session | 885 | 922 | 2,236 |
| native | new session in an existing container | — | 26.2 | 44.7 |
| wasm | new session in an existing container | — | 60.1 | 65.9 |
| child | new session in an existing container | — | 47.1 | 84.4 |

## Host calls [M]

| mode | host call round trip (µs) | loop with host call (µs/iter) | loop with Python call (µs/iter) | codemode cell, 400 calls (ms) | codemode µs per call |
|---|---|---|---|---|---|
| native | 0.10 | 0.13 | 0.03 | 1.06 | 2.65 |
| wasm | 1.42 | 1.48 | 0.06 | 2.79 | 6.98 |
| child | 10.44 | 10.47 | 0.03 | 5.80 | 14.49 |

## Bench programs, ms per run (median) [M]

| program | native | wasm | wasm-plain | wasm-fuel | child |
|---|---|---|---|---|---|
| startup | 0.00 | 0.01 | 0.01 | 0.02 | 0.09 |
| fib | 13.77 | 29.09 | 23.22 | 29.31 | 14.67 |
| loop | 44.63 | 93.18 | 74.77 | 92.21 | 40.95 |
| dict | 63.14 | 116 | 103 | 120 | 64.37 |
| str | 26.27 | 48.08 | 41.93 | 49.92 | 28.07 |
| json_rows | 60.44 | 124 | 106 | 134 | 62.38 |
| closure | 28.87 | 60.24 | 44.71 | 58.42 | 27.80 |
| classes | 74.68 | 171 | 142 | 203 | 73.78 |
| generators | 162 | 309 | 270 | 336 | 154 |
| exceptions | 49.83 | 109 | 98.47 | 125 | 51.70 |
| fmt | 117 | 225 | 195 | 257 | 118 |
| sort | 81.32 | 174 | 146 | 185 | 83.24 |
| interp | 135 | 301 | 250 | 342 | 135 |
| **geomean vs native** (excl. startup) | 1.00× | 2.05× | 1.73× | 2.20× | 1.01× |

## Snapshots [M]

| state | mode | bytes | snapshot (µs) | restore (µs) |
|---|---|---|---|---|
| codemode@call100 | native | 56,001 | 35.0 | 62.7 |
| dicts10k@call | native | 1,891,570 | 1,274 | 517 |
| dicts100k@call | native | 17,999,715 | 14,491 | 11,386 |
| codemode@call100 | wasm | 56,001 | 443 | 527 |
| dicts10k@call | wasm | 1,891,570 | 3,217 | 2,410 |
| dicts100k@call | wasm | 17,999,715 | 42,876 | 29,862 |
| codemode@call100 | child | 56,001 | 351 | 1,306 |
| dicts10k@call | child | 1,891,570 | 3,449 | 4,243 |
| dicts100k@call | child | 17,999,715 | 43,936 | 25,037 |

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
| native | idle | 107 | — | — | — |
| native | active | 564 | — | — | — |
| wasm | idle | 457 | 1,664 | — | — |
| wasm | active | 1,560 | 2,560 | — | — |
| child | idle | 1.3 | — | 4,164 | 392 |
| child | active | 0.0 | — | 5,356 | 1,037 |

## Many sessions on one host (codemode cells) [M]

| mode | sessions | threads | cells | errors | cells/s | p50 (ms) | p99 (ms) | host RSS (MiB) | workers PSS (MiB) |
|---|---|---|---|---|---|---|---|---|---|
| native | 1 | 1 | 256 | 0 | 936 | 1.06 | 1.22 | 7.6 | 0.0 |
| native | 16 | 16 | 256 | 0 | 6,681 | 2.14 | 5.62 | 23.0 | 0.0 |
| native | 64 | 16 | 256 | 0 | 5,479 | 2.17 | 5.76 | 61.4 | 0.0 |
| native | 256 | 16 | 256 | 0 | 3,071 | 5.16 | 6.41 | 212 | 0.0 |
| native | 1024 | 16 | 1024 | 0 | 3,047 | 5.20 | 6.95 | 817 | 0.0 |
| wasm | 1 | 1 | 256 | 0 | 353 | 2.82 | 2.90 | 167 | 0.0 |
| wasm | 16 | 16 | 256 | 0 | 2,708 | 5.69 | 8.18 | 168 | 0.0 |
| wasm | 64 | 16 | 256 | 0 | 2,369 | 5.93 | 12.70 | 244 | 0.0 |
| wasm | 256 | 16 | 256 | 0 | 2,328 | 6.77 | 7.23 | 547 | 0.0 |
| wasm | 1024 | 16 | 1024 | 0 | 2,318 | 6.77 | 7.16 | 1,763 | 0.0 |
| child | 1 | 1 | 256 | 0 | 175 | 5.73 | 6.14 | 138 | 3.5 |
| child | 16 | 16 | 256 | 0 | 1,358 | 11.62 | 12.93 | 137 | 21.6 |
| child | 64 | 16 | 256 | 0 | 1,238 | 11.73 | 17.23 | 137 | 77.4 |
| child | 256 | 16 | 256 | 0 | 1,071 | 14.00 | 18.27 | 137 | 296 |
| child | 1024 | 16 | 1024 | 0 | 1,165 | 12.94 | 19.43 | 137 | 1,172 |

## Fault matrix [M]

| scenario | host | victim | victim session after | bystander session | fresh session | time to control |
|---|---|---|---|---|---|---|
| stack-cyclic-dict native thread 256 KiB | **died: killed by SIGABRT** | thread '<unknown>' (627502) has overflowed its stack; fatal runtime error: stack overflow, | lost with the host | lost with the host | — | — |
| stack-cyclic-dict native thread 512 KiB | **died: killed by SIGABRT** | thread '<unknown>' (627513) has overflowed its stack; fatal runtime error: stack overflow, | lost with the host | lost with the host | — | — |
| stack-cyclic-dict native thread 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 2048 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 8192 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 512 KiB | survived | lost: trap: wasm trap: call stack exhausted | lost: instance dead: trap: wasm trap: call stack exhausted | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict child worker stack 256 KiB | survived | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | completed | ok |  |
| stack-cyclic-dict child worker stack 512 KiB | survived | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | completed | ok |  |
| stack-cyclic-dict child worker stack 16384 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 256 KiB | **died: killed by SIGABRT** | thread '<unknown>' (627584) has overflowed its stack; fatal runtime error: stack overflow, | lost with the host | lost with the host | — | — |
| stack-frozenset-600 native thread 512 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 2048 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 8192 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 wasm max_wasm_stack 512 KiB | survived | lost: trap: wasm trap: call stack exhausted | lost: instance dead: trap: wasm trap: call stack exhausted | completed | ok |  |
| stack-frozenset-600 wasm max_wasm_stack 1024 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 wasm max_wasm_stack 4096 KiB | survived | completed: True | completed: 2 | completed | ok |  |
| stack-frozenset-600 child worker stack 256 KiB | survived | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | completed | ok |  |
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
| runaway loop native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 162 ms after start |
| runaway loop native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.004 ms after the stop |
| runaway loop native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.008 ms after the stop |
| runaway loop wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 361 ms after start |
| runaway loop wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.074 ms after the stop |
| runaway loop wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.047 ms after the stop |
| runaway loop child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 237 ms after start |
| runaway loop child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.014 ms after the stop |
| runaway loop child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.094 ms after the stop |
| runaway loop wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| runaway loop wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 143 ms after start |
| runaway loop child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.357 ms after the stop |
| runaway loop child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 1000 ms after start |
| runaway native native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 4103 ms after start |
| runaway native native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 3888.905 ms after the stop |
| runaway native native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 3889.227 ms after the stop |
| runaway native wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 8658 ms after start |
| runaway native wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 8488.949 ms after the stop |
| runaway native wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.057 ms after the stop |
| runaway native child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 4118 ms after start |
| runaway native child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 3909.700 ms after the stop |
| runaway native child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 3940.627 ms after the stop |
| runaway native wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 210 ms after start |
| runaway native wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 59 ms after start |
| runaway native child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.395 ms after the stop |
| runaway native child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 1001 ms after start |
| memory arena 64 MiB native | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB wasm | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB child | survived | MemoryError | completed: 2 | completed | ok |  |
| memory outside arena (completion value) native | survived | completed:  (host peak RSS 488 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) wasm memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 174 MiB) | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (completion value) child | survived | completed:  (host peak RSS 6 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child prlimit=--as=268435456 | survived | lost: worker exited: signal: 6 (SIGABRT) (core dumped) (host peak RSS 5 MiB) | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | completed | ok |  |
| memory outside arena (host-call argument) native | survived | HostCall { call: 1, name: "tool", args: [Array [Number(0), N (host peak RSS 647 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) wasm memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 121 MiB) | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (host-call argument) child | survived | HostCall { call: 1, name: "tool", args: [Array [Number(0), N (host peak RSS 445 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child prlimit=--as=268435456 | survived | lost: worker exited: signal: 6 (SIGABRT) (core dumped) (host peak RSS 5 MiB) | lost: worker exited: signal: 6 (SIGABRT) (core dumped) | completed | ok |  |

