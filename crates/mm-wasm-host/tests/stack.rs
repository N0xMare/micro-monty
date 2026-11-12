//! The guest's shadow stack (crates/mm-wasm-guest/build.rs) holds the
//! deepest legal inputs with room to spare: a stack overflow in wasm is a
//! trap that loses the instance, so the parser's and compiler's nesting
//! limits must stop every input well before the stack ends. Each case runs
//! on a painted instance, must not trap, and must leave at least 10% of the
//! stack untouched. Running code keeps its nesting in the arena, so only
//! source nesting is deep on the Rust stack.
//!
//! Needs a guest: `MM_WASM_GUEST`, else the root workspace's release build
//! for wasm32-unknown-unknown (`cargo xtask wasm-host` runs it on both
//! shipped guests).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests"
)]

use std::path::PathBuf;

use micro_monty::{Limits, Progress};
use mm_wasm_host::{Runtime, RuntimeConfig, SessionOptions};

fn guest() -> PathBuf {
    std::env::var_os("MM_WASM_GUEST").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/release/mm_wasm_guest.wasm")
        },
        PathBuf::from,
    )
}

/// `wrap` applied `n` times around `core`.
fn nest(wrap: &str, core: &str, n: usize) -> String {
    (0..n).fold(core.to_owned(), |e, _| wrap.replace("{}", &e))
}

/// Indented `head` blocks `n` deep around `pass`.
fn blocks(head: &str, n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        s += &format!("{:i$}{head}\n", "");
    }
    s + &format!("{:n$}pass\n", "")
}

#[test]
fn the_deepest_legal_inputs_leave_a_tenth_of_the_stack() {
    // (what, source, how it ends: "" for completion, else the type name).
    let cases: Vec<(&str, String, &str)> = vec![
        // The deepest on the stack [M]: a lambda per level, each in the
        // parentheses of the previous one's default (about 3.4 KB a level).
        (
            "lambda defaults at 195 levels",
            format!("x = {}\n", nest("(lambda q={}: q)", "1", 195)),
            "",
        ),
        // One past the limit: the parser descends as far before refusing.
        (
            "lambda defaults past the limit",
            format!("x = {}\n", nest("(lambda q={}: q)", "1", 200)),
            "RecursionError",
        ),
        (
            "yield at the limit",
            format!("def g():\n    return {}\n", nest("(yield {})", "1", 199)),
            "RecursionError",
        ),
        (
            "dicts at the limit",
            format!("x = {}\n", nest("{1: {}}", "1", 199)),
            "RecursionError",
        ),
        (
            "parentheses at the bracket limit",
            format!("x = {}\n", nest("({})", "1", 199)),
            "",
        ),
        ("blocks at the indentation limit", blocks("if 1:", 99), ""),
        ("defs at the indentation limit", blocks("def f():", 99), ""),
        // Running code: Python recursion and the walks over nested data
        // stay in the arena.
        (
            "recursion at the limit, deep data walks",
            "import json, copy\n\
             def f(n):\n    return f(n - 1) + 1 if n else 0\n\
             f(990)\n\
             x = []\n\
             for i in range(100000):\n    x = [x]\n\
             for walk in (repr, json.dumps, copy.deepcopy, lambda v: v == v, hash):\n\
             \x20   try:\n        walk(x)\n    except (RecursionError, TypeError):\n        pass\n"
                .to_owned(),
            "",
        ),
    ];
    let config = RuntimeConfig {
        paint_stack: true,
        ..RuntimeConfig::default()
    };
    let rt = Runtime::new(&guest(), config).expect("guest");
    let options = SessionOptions::new().with_limits(Limits::new().with_recursion_depth(1000));
    let mut worst = 0;
    for (what, src, want) in cases {
        let mut s = rt.session(&options).expect("session");
        s.set_output(Box::new(|_, _| {}));
        let end = match s.start(&src, "deep.py") {
            Ok(Progress::Complete(_)) => String::new(),
            Ok(Progress::Error(e)) => e.type_name,
            Ok(p) => panic!("{what}: {p:?}"),
            Err(mm_wasm_host::Error::Compile(e)) => e.type_name,
            Err(e) => panic!("{what}: {e}"),
        };
        assert!(!s.is_dead(), "{what}: the instance trapped");
        assert_eq!(end, want, "{what}");
        let (used, size) = s.stack_high_water().expect("a painted stack");
        println!("{what}: {used} of {size} stack bytes");
        assert!(
            used * 10 <= size * 9,
            "{what}: {used} of {size} stack bytes, more than 90%"
        );
        worst = worst.max(used);
    }
    println!("deepest: {worst} bytes");
}
