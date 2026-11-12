//! The compiler (docs/spec.md §5, §13): source to bytecode through
//! `Engine::compile` (parse, compile, the rendered `CompileError` and its
//! warnings) must never panic or overflow the stack, every error is a
//! Python exception type, and what compiles verifies (`Code::verify`, the
//! check a restored snapshot's code must pass) and survives its own
//! encoding (`Program::encode`, `Program::decode`) unchanged.
//!
//! The input is the `parse` target's (`parse_input::source`), so the two
//! share corpora; it runs on the same 8 MiB thread.
//!
//! ```bash
//! cd fuzz && cargo +nightly fuzz run compile
//! ```

#![no_main]

#[path = "parse_input.rs"]
mod parse_input;

use mm_compile::bytecode::Program;
use std::sync::OnceLock;

fn engine() -> &'static micro_monty::Engine {
    static ENGINE: OnceLock<micro_monty::Engine> = OnceLock::new();
    ENGINE.get_or_init(|| micro_monty::Engine::new(micro_monty::EngineConfig::default()))
}

fn check(src: String) {
    let result = std::thread::Builder::new()
        .stack_size(parse_input::STACK)
        .spawn(move || match engine().compile(&src, "fuzz.py") {
            Err(e) => {
                assert!(
                    matches!(
                        e.type_name.as_str(),
                        "SyntaxError" | "IndentationError" | "TabError" | "RecursionError"
                    ),
                    "{e:?}"
                );
                assert!(e.rendered.ends_with('\n'), "{e:?}");
            }
            Ok(code) => {
                let _ = code.warnings();
                // The same pipeline below the facade, for the program itself.
                let module = mm_parse::parse(&src).expect("parses as in Engine::compile");
                let program = mm_compile::compile(
                    &module,
                    mm_compile::Options {
                        last_expr_value: true,
                    },
                )
                .expect("compiles as in Engine::compile");
                for (i, c) in program.codes.iter().enumerate() {
                    assert!(c.verify(), "code {i} does not verify for {src:?}");
                }
                let bytes = program.encode();
                let back = Program::decode(&bytes).expect("own encoding decodes");
                // Bytes, not `==`: a NaN constant is not equal to itself.
                assert!(back.encode() == bytes, "encoding changes the program for {src:?}");
            }
        })
        .expect("spawn")
        .join();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Some(src) = parse_input::source(data) {
        check(src);
    }
});
