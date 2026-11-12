//! The parser (docs/spec.md §5, §13): `mm_parse::parse` on arbitrary bytes
//! must never panic or overflow the stack, and every error is a Python
//! exception type. The parse runs on its own thread
//! (`parse_input::STACK`, 8 MiB, which fits the nesting limit under ASan).
//!
//! The first byte picks the mode: the rest as source; or the rest wrapped
//! in deep nesting (brackets, lambdas, `not`, f-strings, indentation) so
//! inputs reach the limits quickly.
//!
//! With `--features ruff-oracle` the target is also differential against
//! the phase-1 ruff front end: where both accept, the ASTs must be equal;
//! where both reject with a micro-monty error (an unsupported construct),
//! the errors must be equal. (Where only one rejects, CPython decides;
//! ruff 0.0.14 accepts some invalid indentation and rejects `raise;x`.)
//!
//! ```bash
//! cd fuzz && cargo +nightly fuzz run parse
//! cd fuzz && cargo +nightly fuzz run parse --features ruff-oracle
//! ```

#![no_main]

#[path = "parse_input.rs"]
mod parse_input;

fn check(src: String) {
    let result = std::thread::Builder::new()
        .stack_size(parse_input::STACK)
        .spawn(move || {
            let own = mm_parse::parse(&src);
            if let Err(e) = &own {
                assert!(
                    matches!(
                        e.kind,
                        "SyntaxError" | "IndentationError" | "TabError" | "RecursionError"
                    ),
                    "{e:?}"
                );
            }
            #[cfg(feature = "ruff-oracle")]
            differential(&src, &own);
        })
        .expect("spawn")
        .join();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[cfg(feature = "ruff-oracle")]
fn differential(
    src: &str,
    own: &Result<mm_compile::ast::Module, mm_compile::CompileError>,
) {
    if src.contains("\\N") {
        // `\N{...}` is unsupported on purpose.
        return;
    }
    let lowering = |m: &str| m.starts_with("micro-monty does not support") || m.starts_with("future feature");
    match (own, mm_parse::ruff::parse(src)) {
        (Ok(a), Ok(b)) => assert!(*a == b, "ASTs differ for {src:?}"),
        (Err(a), Err(b)) if lowering(&a.msg) && lowering(&b.msg) => {
            assert_eq!((a.kind, &a.msg, a.pos), (b.kind, &b.msg, b.pos), "{src:?}");
        }
        _ => {}
    }
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Some(src) = parse_input::source(data) {
        check(src);
    }
});
