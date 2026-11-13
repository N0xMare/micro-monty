//! The binary the size budget measures (`cargo xtask size`): the whole engine
//! (parser, compiler, VM, prelude, snapshots) running one cell.

use micro_monty::{Engine, EngineConfig, Progress, Session};

// std's allocator without its per-site checks (crates/mm-wasm-alloc,
// docs/user-guide.md); optional for embedders.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[global_allocator]
static ALLOCATOR: mm_wasm_alloc::Allocator = mm_wasm_alloc::Allocator::new();

fn main() {
    let engine = Engine::new(EngineConfig::default());
    let source = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "print('hello')".to_owned());
    let code = match engine.compile(&source, "<hello>") {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let Ok(mut session) = Session::new(&engine) else {
        std::process::exit(1);
    };
    session.set_output(Box::new(|text| print!("{text}")));
    match session.start(&code) {
        Progress::Complete(_) => {}
        Progress::Error(error) => {
            eprint!("{}", error.traceback);
            std::process::exit(1);
        }
        _ => std::process::exit(2),
    }
    let Ok(snapshot) = session.snapshot() else {
        std::process::exit(1);
    };
    if engine.restore(&snapshot).is_err() {
        std::process::exit(1);
    }
}
