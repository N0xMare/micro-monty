//! The `parse` fuzz target's input decoding and parse thread, shared with
//! the regression test that replays its seeds
//! (`crates/micro-monty/tests/session.rs`), so a seed means the same source
//! on the same stack in both.

/// The parse thread's stack. 8 MiB: with the address sanitizer a nesting level takes about
/// 30 KB, so 1 MiB overflowed at 35 levels, well inside the 200-level
/// limit (a release build fits 200 levels in 512 KiB). Unbounded recursion
/// still overflows it within the input lengths the fuzzer reaches.
pub const STACK: usize = 8 << 20;

/// The source one fuzz input stands for: the first byte picks the mode,
/// the rest as source or wrapped in deep nesting.
pub fn source(data: &[u8]) -> Option<String> {
    let (&mode, rest) = data.split_first()?;
    let body = String::from_utf8_lossy(rest).into_owned();
    Some(if mode < 0xc0 {
        body
    } else {
        wrap(mode.wrapping_mul(7), &body)
    })
}

fn wrap(mode: u8, body: &str) -> String {
    let n = 150 + usize::from(mode & 0x3f);
    match mode >> 6 {
        0 => format!("x = {}{body}{}\n", "([{".repeat(n / 3), "}])".repeat(n / 3)),
        1 => format!("x = {}{body}\n", "lambda: not -".repeat(n)),
        2 => {
            let mut s = String::from(body);
            for _ in 0..n / 2 {
                s = format!("f'{{{s}}}'");
            }
            format!("x = {s}\n")
        }
        _ => {
            let mut s = String::new();
            for i in 0..n.min(110) {
                s.push_str(&" ".repeat(i));
                s.push_str("if x:\n");
            }
            s.push_str(&" ".repeat(n.min(110)));
            s.push_str(body);
            s
        }
    }
}
