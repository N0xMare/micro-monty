## Startup [M]

| cold: mode | main → first cell done (ms) | module load (ms) | spawn → exit (ms) |
|---|---|---|---|
| native | 0.26 | 0.00 | 2.15 |
| wasm | 1.47 | 0.40 | 3.88 |
| wasm-compile | 321 | 320 | 334 |
| child | 2.06 | 0.00 | 4.03 |

| warm: mode | what | session ready (µs) | first cell done (µs, median) | p99 (µs) |
|---|---|---|---|---|
| native | new container + session | 40.7 | 44.2 | 96.9 |
| wasm | new container + session | 179 | 196 | 277 |
| wasm-pool | new container + session | 175 | 192 | 268 |
| child | new container + session | 1,790 | 1,941 | 3,440 |
| native | new session in an existing container | — | 91.7 | 166 |
| wasm | new session in an existing container | — | 223 | 339 |
| child | new session in an existing container | — | 162 | 257 |

## Host calls [M]

| mode | host call round trip (µs) | loop with host call (µs/iter) | loop with Python call (µs/iter) | codemode cell, 400 calls (ms) | codemode µs per call |
|---|---|---|---|---|---|
| native | 0.17 | 0.22 | 0.05 | 1.97 | 4.93 |
| wasm | 2.45 | 2.55 | 0.10 | 5.47 | 13.67 |
| child | 52.03 | 52.08 | 0.05 | 25.96 | 64.91 |

## Bench programs, ms per run (median) [M]

| program | native | wasm | wasm-plain | wasm-fuel | child |
|---|---|---|---|---|---|
| startup | 0.01 | 0.03 | 0.02 | 0.03 | 0.20 |
| fib | 25.74 | 56.36 | 49.58 | 54.44 | 30.32 |
| loop | 79.59 | 150 | 115 | 155 | 70.60 |
| dict | 157 | 245 | 209 | 248 | 127 |
| str | 50.78 | 91.36 | 85.93 | 96.05 | 51.14 |
| json_rows | 112 | 245 | 211 | 256 | 109 |
| closure | 49.79 | 106 | 81.79 | 107 | 45.24 |
| classes | 144 | 335 | 252 | 364 | 147 |
| generators | 286 | 618 | 508 | 669 | 287 |
| exceptions | 98.27 | 213 | 176 | 232 | 100 |
| fmt | 234 | 456 | 389 | 511 | 235 |
| sort | 181 | 370 | 292 | 388 | 167 |
| interp | 269 | 607 | 473 | 647 | 264 |
| **geomean vs native** (excl. startup) | 1.00× | 2.04× | 1.68× | 2.15× | 0.97× |

## Snapshots [M]

| state | mode | bytes | snapshot (µs) | restore (µs) |
|---|---|---|---|---|
| codemode@call100 | native | 56,049 | 61.6 | 111 |
| dicts10k@call | native | 1,891,618 | 1,856 | 920 |
| dicts100k@call | native | 17,999,811 | 25,517 | 19,749 |
| codemode@call100 | wasm | 56,049 | 621 | 699 |
| dicts10k@call | wasm | 1,891,618 | 5,178 | 3,602 |
| dicts100k@call | wasm | 17,999,811 | 67,488 | 48,566 |
| codemode@call100 | child | 56,049 | 598 | 2,095 |
| dicts10k@call | child | 1,891,618 | 5,679 | 5,838 |
| dicts100k@call | child | 17,999,811 | 71,077 | 45,023 |

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
| native | active | 570 | — | — | — |
| wasm | idle | 450 | 1,664 | — | — |
| wasm | active | 1,558 | 2,560 | — | — |
| child | idle | 1.4 | — | 1,060 | 393 |
| child | active | 0.0 | — | 3,448 | 1,036 |

## Many sessions on one host (codemode cells) [M]

| mode | sessions | threads | cells | errors | cells/s | p50 (ms) | p99 (ms) | host RSS (MiB) | workers PSS (MiB) |
|---|---|---|---|---|---|---|---|---|---|
| native | 1 | 1 | 256 | 0 | 482 | 2.03 | 2.84 | 6.4 | 0.0 |
| native | 16 | 16 | 256 | 0 | 3,288 | 4.01 | 12.03 | 22.8 | 0.0 |
| native | 64 | 16 | 256 | 0 | 2,674 | 4.08 | 15.15 | 61.3 | 0.0 |
| native | 256 | 16 | 256 | 0 | 1,414 | 10.39 | 17.64 | 212 | 0.0 |
| native | 1024 | 16 | 1024 | 0 | 1,502 | 10.01 | 16.89 | 822 | 0.0 |
| wasm | 1 | 1 | 256 | 0 | 180 | 5.53 | 5.99 | 160 | 0.0 |
| wasm | 16 | 16 | 256 | 0 | 1,435 | 5.85 | 14.51 | 163 | 0.0 |
| wasm | 64 | 16 | 256 | 0 | 1,790 | 5.65 | 12.78 | 240 | 0.0 |
| wasm | 256 | 16 | 256 | 0 | 1,506 | 7.37 | 13.49 | 544 | 0.0 |
| wasm | 1024 | 16 | 1024 | 0 | 1,819 | 7.26 | 14.40 | 1,761 | 0.0 |
| child | 1 | 1 | 256 | 0 | 39.0 | 25.45 | 28.00 | 134 | 3.5 |
| child | 16 | 16 | 256 | 0 | 707 | 13.32 | 35.36 | 132 | 21.9 |
| child | 64 | 16 | 256 | 0 | 496 | 31.31 | 39.48 | 132 | 77.4 |
| child | 256 | 16 | 256 | 0 | 681 | 17.86 | 37.33 | 132 | 294 |
| child | 1024 | 16 | 1024 | 0 | 533 | 29.37 | 38.22 | 132 | 1,165 |

## Fault matrix [M]

| scenario | host | victim | victim session after | bystander session | fresh session | time to control |
|---|---|---|---|---|---|---|
| stack-cyclic-dict native thread 256 KiB | **died: killed by SIGABRT** | thread '<unknown>' (32994) has overflowed its stack; fatal runtime error: stack overflow,  | lost with the host | lost with the host | — | — |
| stack-cyclic-dict native thread 512 KiB | **died: killed by SIGABRT** | thread '<unknown>' (32996) has overflowed its stack; fatal runtime error: stack overflow,  | lost with the host | lost with the host | — | — |
| stack-cyclic-dict native thread 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 2048 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict native thread 8192 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 512 KiB | survived | lost: trap: wasm trap: call stack exhausted | lost: instance dead: trap: wasm trap: call stack exhausted | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 1024 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict wasm max_wasm_stack 4096 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-cyclic-dict child worker stack 256 KiB | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| stack-cyclic-dict child worker stack 512 KiB | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| stack-cyclic-dict child worker stack 16384 KiB | survived | completed: RecursionError | completed: 2 | completed | ok |  |
| stack-frozenset-600 native thread 256 KiB | **died: killed by SIGABRT** | thread '<unknown>' (33046) has overflowed its stack; fatal runtime error: stack overflow,  | lost with the host | lost with the host | — | — |
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
| panic native shipped (immediate-abort) | **died: killed by SIGILL** |  | lost with the host | lost with the host | — | — |
| panic wasm release guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic wasm shipped guest | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| panic child release worker (panic=abort) | survived | lost: worker exited: signal: 6 (SIGABRT) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| panic child release-unwind worker | survived | SystemError: micro-monty internal error (engine panic: test hoo | SystemError: micro-monty internal error (engine panic: test hoo | completed | ok |  |
| panic child shipped worker | survived | lost: worker exited: signal: 4 (SIGILL) | lost: worker exited: signal: 4 (SIGILL) | completed | ok |  |
| runaway loop native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 345 ms after start |
| runaway loop native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.063 ms after the stop |
| runaway loop native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.066 ms after the stop |
| runaway loop wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 588 ms after start |
| runaway loop wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.043 ms after the stop |
| runaway loop wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.099 ms after the stop |
| runaway loop child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 314 ms after start |
| runaway loop child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 0.040 ms after the stop |
| runaway loop child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 0.192 ms after the stop |
| runaway loop wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 209 ms after start |
| runaway loop wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 234 ms after start |
| runaway loop child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.455 ms after the stop |
| runaway loop child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 998 ms after start |
| runaway native native engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7366 ms after start |
| runaway native native fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7175.455 ms after the stop |
| runaway native native async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7229.275 ms after the stop |
| runaway native wasm engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 16238 ms after start |
| runaway native wasm fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 16129.259 ms after the stop |
| runaway native wasm async stop at 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 0.162 ms after the stop |
| runaway native child engine fuel 50M | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7392 ms after start |
| runaway native child fuel slices 10k + host flag | survived | Parked { reason: "fuel" } | completed: 2 | completed | ok | 7213.210 ms after the stop |
| runaway native child async stop at 200 ms | survived | Parked { reason: "requested" } | completed: 2 | completed | ok | 7225.590 ms after the stop |
| runaway native wasm epoch budget 200 ms | survived | lost: trap: wasm trap: interrupt | lost: instance dead: trap: wasm trap: interrupt | completed | ok | 209 ms after start |
| runaway native wasm-fuel 2e9 | survived | lost: trap: wasm trap: all fuel consumed by WebAssembly | lost: instance dead: trap: wasm trap: all fuel consumed by WebAssembly | completed | ok | 121 ms after start |
| runaway native child SIGKILL at 200 ms | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 0.747 ms after the stop |
| runaway native child RLIMIT_CPU 1 s | survived | lost: worker exited: signal: 9 (SIGKILL) | lost: worker exited: signal: 9 (SIGKILL) | completed | ok | 1000 ms after start |
| memory arena 64 MiB native | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB wasm | survived | MemoryError | completed: 2 | completed | ok |  |
| memory arena 64 MiB child | survived | MemoryError | completed: 2 | completed | ok |  |
| memory outside arena (completion value) native | survived | completed:  (host peak RSS 487 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) wasm memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 171 MiB) | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (completion value) child | survived | completed:  (host peak RSS 4 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (completion value) child prlimit=--as=268435456 | survived | lost: worker exited: signal: 6 (SIGABRT) (host peak RSS 2 MiB) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |
| memory outside arena (host-call argument) native | survived | HostCall { call: 1, name: "tool", args: [Array [Number(0), N (host peak RSS 645 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) wasm memory_limit_mib=256 | survived | lost: trap: wasm trap: wasm `unreachable` instruction executed (host peak RSS 117 MiB) | lost: instance dead: trap: wasm trap: wasm `unreachable` instruction executed | completed | ok |  |
| memory outside arena (host-call argument) child | survived | HostCall { call: 1, name: "tool", args: [Array [Number(0), N (host peak RSS 443 MiB) | completed: 2 | completed | ok |  |
| memory outside arena (host-call argument) child prlimit=--as=268435456 | survived | lost: worker exited: signal: 6 (SIGABRT) (host peak RSS 2 MiB) | lost: worker exited: signal: 6 (SIGABRT) | completed | ok |  |

