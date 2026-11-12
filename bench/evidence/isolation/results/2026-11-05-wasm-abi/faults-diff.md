| row | what changed |
|---|---|
| stack-cyclic-dict native thread 256 KiB | host: died: killed by SIGABRT → survived; victim: None → completed: RecursionError; after: None → completed: 2; bystander: None → completed |
| stack-cyclic-dict native thread 512 KiB | host: died: killed by SIGABRT → survived; victim: None → completed: RecursionError; after: None → completed: 2; bystander: None → completed |
| stack-cyclic-dict wasm max_wasm_stack 512 KiB | victim: lost: trap: wasm trap: call stack exhausted → completed: RecursionError; after: lost: instance dead: trap: wasm trap: call stack exhaust → completed: 2 |
| stack-cyclic-dict child worker stack 256 KiB | victim: lost: worker exited: signal: 6 (SIGABRT) → completed: RecursionError; after: lost: worker exited: signal: 6 (SIGABRT) → completed: 2 |
| stack-cyclic-dict child worker stack 512 KiB | victim: lost: worker exited: signal: 6 (SIGABRT) → completed: RecursionError; after: lost: worker exited: signal: 6 (SIGABRT) → completed: 2 |
| stack-frozenset-600 native thread 256 KiB | host: died: killed by SIGABRT → survived; victim: None → completed: True; after: None → completed: 2; bystander: None → completed |
| stack-frozenset-600 wasm max_wasm_stack 512 KiB | victim: lost: trap: wasm trap: call stack exhausted → completed: True; after: lost: instance dead: trap: wasm trap: call stack exhaust → completed: 2 |
| stack-frozenset-600 child worker stack 256 KiB | victim: lost: worker exited: signal: 6 (SIGABRT) → completed: True; after: lost: worker exited: signal: 6 (SIGABRT) → completed: 2 |
| stack-cyclic-dict abi max_wasm_stack 512 KiB | victim: lost: trap: wasm trap: call stack exhausted → completed: RecursionError; after: lost: trap: wasm trap: call stack exhausted → completed: 2 |
| stack-frozenset-600 abi max_wasm_stack 512 KiB | victim: lost: trap: wasm trap: call stack exhausted → completed: True; after: lost: trap: wasm trap: call stack exhausted → completed: 2 |
| runaway native abi engine fuel 50M | time to control: 16087.2 → 0.1 ms |
| runaway native abi fuel slices 10k + host flag | time to control: 15846.5 → 6384.7 ms |
| stack-cyclic-dict abi-uu max_wasm_stack 512 KiB | victim: lost: trap: wasm trap: call stack exhausted → completed: RecursionError; after: lost: trap: wasm trap: call stack exhausted → completed: 2 |
| stack-frozenset-600 abi-uu max_wasm_stack 512 KiB | victim: lost: trap: wasm trap: call stack exhausted → completed: True; after: lost: trap: wasm trap: call stack exhausted → completed: 2 |
| runaway native abi-uu engine fuel 50M | time to control: 15682.7 → 0.0 ms |
| runaway native abi-uu fuel slices 10k + host flag | time to control: 15462.2 → 6383.6 ms |
| runaway native native engine fuel 50M | time to control: 7402.6 → 0.0 ms |
| runaway native native fuel slices 10k + host flag | time to control: 7063.5 → 2805.8 ms |
| runaway native wasm engine fuel 50M | time to control: 16382.2 → 0.1 ms |
| runaway native wasm fuel slices 10k + host flag | time to control: 16172.8 → 6365.4 ms |
| runaway native child engine fuel 50M | time to control: 7511.7 → 0.2 ms |
| runaway native child fuel slices 10k + host flag | time to control: 7144.3 → 2865.1 ms |
| memory outside arena (completion value) native | host peak RSS: 487 → 114 MiB |
| memory outside arena (completion value) wasm memory_limit_mib=256 | victim: lost: trap: wasm trap: wasm `unreachable` instruction ex → completed: ; after: lost: instance dead: trap: wasm trap: wasm `unreachable` → completed: 2 |
| memory outside arena (completion value) abi memory_limit_mib=256 | victim: lost: trap: wasm trap: wasm `unreachable` instruction ex → completed: ; after: lost: trap: wasm trap: wasm `unreachable` instruction ex → completed: 2 |
| memory outside arena (completion value) abi-uu memory_limit_mib=256 | victim: lost: trap: wasm trap: wasm `unreachable` instruction ex → completed: ; after: lost: trap: wasm trap: wasm `unreachable` instruction ex → completed: 2 |
| memory outside arena (completion value) child prlimit=--as=268435456 | victim: lost: worker exited: signal: 6 (SIGABRT) → completed: ; after: lost: worker exited: signal: 6 (SIGABRT) → completed: 2 |
| memory outside arena (host-call argument) native | victim: HostCall { call: 1, name: "tool", args:  → MemoryError; host peak RSS: 645 → 111 MiB |
| memory outside arena (host-call argument) wasm memory_limit_mib=256 | victim: lost: trap: wasm trap: wasm `unreachable` instruction ex → MemoryError; after: lost: instance dead: trap: wasm trap: wasm `unreachable` → completed: 2 |
| memory outside arena (host-call argument) abi memory_limit_mib=256 | victim: lost: trap: wasm trap: wasm `unreachable` instruction ex → MemoryError; after: lost: trap: wasm trap: wasm `unreachable` instruction ex → completed: 2 |
| memory outside arena (host-call argument) abi-uu memory_limit_mib=256 | victim: lost: trap: wasm trap: wasm `unreachable` instruction ex → MemoryError; after: lost: trap: wasm trap: wasm `unreachable` instruction ex → completed: 2 |
| memory outside arena (host-call argument) child | victim: HostCall { call: 1, name: "tool", args:  → MemoryError; host peak RSS: 443 → 2 MiB |
| memory outside arena (host-call argument) child prlimit=--as=268435456 | victim: lost: worker exited: signal: 6 (SIGABRT) → MemoryError; after: lost: worker exited: signal: 6 (SIGABRT) → completed: 2 |
