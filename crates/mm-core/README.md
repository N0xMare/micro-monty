# mm-core

micro-monty heap: NaN-boxed values, arena, tracing GC, write barrier, interning.

An internal crate of [micro-monty](https://crates.io/crates/micro-monty),
published only because the facade depends on it. Its API has no semver
guarantees and changes with every release: depend on `micro-monty`, never
on this crate. See the [repository](https://github.com/N0xMare/micro-monty)
for the design and the licence.
