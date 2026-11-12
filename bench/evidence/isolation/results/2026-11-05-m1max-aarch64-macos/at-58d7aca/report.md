## Startup [M]

| cold: mode | main → first cell done (ms) | module load (ms) | spawn → exit (ms) |
|---|---|---|---|
| native | 0.16 | 0.00 | 3.40 |
| wasm | 1.93 | 1.23 | 5.37 |
| wasm-compile | 231 | 230 | 236 |
| child | 2.39 | 0.00 | 5.84 |

| warm: mode | what | session ready (µs) | first cell done (µs, median) | p99 (µs) |
|---|---|---|---|---|
| native | new container + session | 30.8 | 33.1 | 48.4 |
| wasm | new container + session | 98.7 | 104 | 173 |
| wasm-pool | new container + session | 99.0 | 104 | 218 |
| child | new container + session | 2,271 | 2,335 | 6,634 |
| native | new session in an existing container | — | 35.2 | 57.4 |
| wasm | new session in an existing container | — | 71.0 | 120 |
| child | new session in an existing container | — | 55.0 | 115 |

## Host calls [M]

| mode | host call round trip (µs) | loop with host call (µs/iter) | loop with Python call (µs/iter) | codemode cell, 400 calls (ms) | codemode µs per call |
|---|---|---|---|---|---|
| native | 0.12 | 0.15 | 0.03 | 1.15 | 2.86 |
| wasm | 1.30 | 1.36 | 0.06 | 2.83 | 7.08 |
| child | 8.27 | 8.30 | 0.03 | 5.66 | 14.15 |

## Bench programs, ms per run (median) [M]

| program | native | wasm | wasm-plain | wasm-fuel | child |
|---|---|---|---|---|---|
| startup | 0.01 | 0.01 | 0.01 | 0.01 | 0.05 |
| fib | 19.61 | 36.84 | 29.54 | 38.71 | 19.78 |
| loop | 45.39 | 88.42 | 77.75 | 93.55 | 45.42 |
| dict | 60.75 | 120 | 104 | 130 | 64.37 |
| str | 24.17 | 46.74 | 40.69 | 51.30 | 25.60 |
| json_rows | 57.27 | 126 | 105 | 138 | 62.81 |
| closure | 32.33 | 58.93 | 46.26 | 61.10 | 32.29 |
| classes | 86.10 | 161 | 134 | 172 | 88.08 |
| generators | 161 | 376 | 304 | 377 | 163 |
| exceptions | 53.62 | 108 | 92.66 | 113 | 54.52 |
| fmt | 114 | 208 | 170 | 228 | 118 |
| sort | 109 | 182 | 162 | 211 | 105 |
| interp | 158 | 326 | 265 | 346 | 161 |
| **geomean vs native** (excl. startup) | 1.00× | 1.95× | 1.64× | 2.09× | 1.02× |

## Snapshots [M]

| state | mode | bytes | snapshot (µs) | restore (µs) |
|---|---|---|---|---|
| codemode@call100 | native | 56,001 | 44.2 | 80.9 |
| dicts10k@call | native | 1,891,570 | 1,022 | 687 |
| dicts100k@call | native | 17,999,715 | 11,277 | 6,366 |
| codemode@call100 | wasm | 56,001 | 297 | 372 |
| dicts10k@call | wasm | 1,891,570 | 2,269 | 1,692 |
| dicts100k@call | wasm | 17,999,715 | 21,735 | 13,948 |
| codemode@call100 | child | 56,001 | 293 | 2,322 |
| dicts10k@call | child | 1,891,570 | 1,970 | 3,666 |
| dicts100k@call | child | 17,999,715 | 20,129 | 15,517 |

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
| native | idle | 117 | — | — | — |
| native | active | 574 | — | — | — |
| wasm | idle | 389 | 1,664 | — | — |
| wasm | active | 1,441 | 2,560 | — | — |
| child | idle | 8.0 | — | 2,944 | 1,504 |
| child | active | 1.1 | — | 4,800 | 2,912 |

## Many sessions on one host (codemode cells) [M]

| mode | sessions | threads | cells | errors | cells/s | p50 (ms) | p99 (ms) | host RSS (MiB) | workers phys_footprint (MiB) |
|---|---|---|---|---|---|---|---|---|---|
| native | 1 | 1 | 256 | 0 | 876 | 1.11 | 1.48 | 6.1 | 0.0 |
| native | 16 | 10 | 256 | 0 | 3,824 | 1.65 | 6.15 | 22.0 | 0.0 |
| native | 64 | 10 | 256 | 0 | 3,896 | 1.63 | 6.92 | 59.2 | 0.0 |
| native | 256 | 10 | 256 | 0 | 3,581 | 1.80 | 6.55 | 201 | 0.0 |
| native | 1024 | 10 | 1024 | 0 | 4,403 | 1.58 | 7.51 | 777 | 0.0 |
| wasm | 1 | 1 | 256 | 0 | 337 | 2.93 | 3.56 | 784 | 0.0 |
| wasm | 16 | 10 | 256 | 0 | 1,844 | 3.05 | 10.02 | 807 | 0.0 |
| wasm | 64 | 10 | 256 | 0 | 2,073 | 3.05 | 13.95 | 879 | 0.0 |
| wasm | 256 | 10 | 256 | 0 | 2,118 | 3.44 | 15.32 | 1,168 | 0.0 |
| wasm | 1024 | 10 | 1024 | 0 | 1,933 | 3.55 | 16.94 | 2,323 | 0.0 |
| child | 1 | 1 | 256 | 0 | 176 | 5.48 | 7.68 | 816 | 3.9 |
| child | 16 | 10 | 256 | 0 | 282 | 28.05 | 151 | 816 | 54.1 |
| child | 64 | 10 | 256 | 0 | 273 | 36.18 | 51.74 | 816 | 203 |
| child | 256 | 10 | 256 | 0 | 217 | 37.92 | 83.26 | 816 | 769 |
| child | 1024 | 10 | 1024 | 0 | 292 | 33.13 | 53.50 | 474 | 3,084 |

## Fault matrix [M]

| scenario | host | victim | victim session after | bystander session | fresh session | time to control |
|---|---|---|---|---|---|---|
| stack-cyclic-dict native thread 256 KiB | **died: killed by SIGABRT** | thread '<unknown>' (362748134) has overflowed its stack; fatal runtime error: stack overfl | lost with the host | lost with the host | — | — |
| stack-cyclic-dict native thread 512 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 2048 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 8192 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 512 KiB | survived | lost: trap: wasm trap: call stack exhausted | lost: instance dead: trap: wasm trap: call stack exhausted | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict child worker stack 256 KiB | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| stack-cyclic-dict child worker stack 512 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict child worker stack 16384 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 256 KiB | **died: killed by SIGABRT** | thread '<unknown>' (362748222) has overflowed its stack; fatal runtime error: stack overfl | lost with the host | lost with the host | — | — |
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
| panic native release (panic=abort) | **died: killed by SIGABRT** | test hook: injected panic in start; note: run with `RUST_BACKTRACE=1` environment variable | lost with the host | lost with the host | — | — |
| panic native release-unwind | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic native shipped (immediate-abort) | **died: killed by SIGTRAP** |  | lost with the host | lost with the host | — | — |
| panic wasm release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic wasm shipped guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic child release worker (panic=abort) | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| panic child release-unwind worker | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic child shipped worker | survived | lost: worker exited: signal: 5 (SIGTRAP) | lost: worker exited: signal: 5 (SIGTRAP) | completed | ok |  |
| runaway loop native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 309 ms after start |
| runaway loop native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.008 ms after the stop |
| runaway loop native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.007 ms after the stop |
| runaway loop wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 489 ms after start |
| runaway loop wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.013 ms after the stop |
| runaway loop wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.495 ms after the stop |
| runaway loop child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 307 ms after start |
| runaway loop child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.025 ms after the stop |
| runaway loop child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.040 ms after the stop |
| runaway loop wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 243 ms after start |
| runaway loop wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 209 ms after start |
| runaway loop child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.535 ms after the stop |
| runaway loop child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 24 (SIGXCPU) | lost: worker exited: signal: 24 (SIGXCPU) | completed | ok | 1003 ms after start |
| runaway native native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7598 ms after start |
| runaway native native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7270.839 ms after the stop |
| runaway native native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7370.397 ms after the stop |
| runaway native wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 12431 ms after start |
| runaway native wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 12145.742 ms after the stop |
| runaway native wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 1.065 ms after the stop |
| runaway native child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7516 ms after start |
| runaway native child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7345.403 ms after the stop |
| runaway native child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7316.503 ms after the stop |
| runaway native wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 244 ms after start |
| runaway native wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 66 ms after start |
| runaway native child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.557 ms after the stop |
| runaway native child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 24 (SIGXCPU) | lost: worker exited: signal: 24 (SIGXCPU) | completed | ok | 1001 ms after start |
| memory arena 64 MiB native | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB wasm | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB child | survived | MemoryError | completed: 2 | completed | ok |  |
| memory outside arena (completion value) native | survived | completed:  (host peak RSS 487 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) wasm memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 170 MiB) | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (completion value) child | survived | completed:  (host peak RSS 5 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child prlimit=--as=268435456 | survived | lost: not started: spawn /Users/local/dev/agents/micro-monty/target/isolation/release/iso-worker: Invalid argument (os error 22) | — | — | — |  |
| memory outside arena (host-call argument) native | survived | HostCall { call: 1, name: "tool", args: [Array [Number(0), N (host peak RSS 774 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) wasm memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 116 MiB) | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (host-call argument) child | survived | HostCall { call: 1, name: "tool", args: [Array [Number(0), N (host peak RSS 519 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child prlimit=--as=268435456 | survived | lost: not started: spawn /Users/local/dev/agents/micro-monty/target/isolation/release/iso-worker: Invalid argument (os error 22) | — | — | — |  |

