//! Property test for big-int arithmetic: random operands of many sizes and
//! shapes go through `mm run` and through CPython 3.14 in one batched
//! program each; every result (and a few algebraic identities) must print
//! identically. The oracle is found as in `conformance_own.rs`.

#![allow(clippy::expect_used, clippy::panic, reason = "test harness")]
#![allow(clippy::doc_markdown, reason = "the docs name CPython in prose")]

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    process::Command,
};

fn python() -> PathBuf {
    if let Some(p) = std::env::var_os("PYTHON") {
        return PathBuf::from(p);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let p = Path::new(&home).join(".local/bin/python3.14");
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("python3.14")
}

/// xorshift64*: deterministic, so a failure reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A signed hex literal of about `bits` bits, in one of several shapes.
    fn operand(&mut self, bits: u64) -> String {
        let digits = bits.div_ceil(4).max(1);
        let mut hex = String::new();
        match self.below(6) {
            // 2**k, 2**k - 1 and 2**k + 1: carries and borrows across limbs.
            0 => {
                hex.push('1');
                hex.push_str(&"0".repeat(usize::try_from(digits).unwrap_or(1)));
            }
            1 => hex.push_str(&"f".repeat(usize::try_from(digits).unwrap_or(1))),
            2 => {
                hex.push('1');
                hex.push_str(&"0".repeat(usize::try_from(digits).unwrap_or(1) - 1));
                hex.push('1');
            }
            _ => {
                for _ in 0..digits {
                    let _ = write!(hex, "{:x}", self.below(16));
                }
            }
        }
        let sign = if self.below(2) == 0 { "-" } else { "" };
        format!("{sign}0x{hex}")
    }
}

const PROGRAM: &str = r"
import math

def show(f):
    try:
        return repr(f())
    except Exception as e:
        return type(e).__name__ + ': ' + str(e)

# Exponents of `pow(a, e, m)` to 1000 bits: one past about 1500 with a
# 4000-bit modulus would take over 50 ms in one step (`MemoryError`).
for a, b in PAIRS:
    s = abs(b) % 300
    e = abs(b) % 13
    m = abs(b) + 2
    print(a + b, a - b, a * b, a & b, a | b, a ^ b, ~a, -a, abs(a))
    print(show(lambda: a // b), show(lambda: a % b), show(lambda: divmod(a, b)))
    print(a << s, a >> s, a >> (s + 2000), show(lambda: a ** e), pow(a, abs(b) % 2**1000 + 1, m))
    print(a < b, a <= b, a == b, a > b, a == a + 0, hash(a), a.bit_length(), a.bit_count())
    print(str(a) == repr(a), int(str(a)) == a, int(hex(a), 16) == a, int(bin(b), 0) == b)
    print(show(lambda: float(a)), show(lambda: a / b), show(lambda: a + 0.5), a == float(a) if a.bit_length() < 1000 else None)
    print(math.gcd(a, b), math.isqrt(abs(a)), show(lambda: pow(a, -1, m)))
    if b:
        q, r = divmod(a, b)
        print(q * b + r == a, (a * b) // b == a, abs(r) < abs(b), (r == 0) or ((r < 0) == (b < 0)))
    print((a + b) - b == a, (a ^ b) ^ b == a, (a & b) | (a & ~b) == a, (a << s) >> s == a)
";

#[test]
fn bigint_ops_match_cpython() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut pairs = String::from("PAIRS = [\n");
    let sizes = [
        1, 8, 31, 32, 33, 47, 48, 63, 64, 65, 96, 128, 200, 640, 1300, 1400, 2700, 4000,
    ];
    for i in 0..240 {
        let ba = sizes[i % sizes.len()];
        let bb = sizes[usize::try_from(rng.below(sizes.len() as u64)).unwrap_or(0)];
        let a = rng.operand(ba);
        let b = rng.operand(bb);
        let _ = writeln!(pairs, "    ({a}, {b}),");
    }
    pairs
        .push_str("    (0, 0), (0, -1), (-1, 0), (1, -(2**64)), (-(2**63), -1), (2**63, 2**63),\n");
    pairs.push_str("    (2**61 - 1, 2**61), (-(2**61), 2**61 + 1), (10**300, 7), (-(10**300), -(2**100)),\n]\n");
    let src = format!("{pairs}{PROGRAM}");
    let dir = std::env::temp_dir().join(format!("mm-bigint-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("bigint_oracle.py");
    std::fs::write(&file, src).expect("write program");
    let mine = Command::new(env!("CARGO_BIN_EXE_mm"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("runs mm");
    let theirs = Command::new(python())
        .arg("-X")
        .arg("utf8")
        .arg(&file)
        .env("PYTHONHASHSEED", "0")
        .output()
        .expect("runs python3.14");
    let _ = std::fs::remove_dir_all(&dir);
    let (m, t) = (
        String::from_utf8_lossy(&mine.stdout),
        String::from_utf8_lossy(&theirs.stdout),
    );
    assert!(
        theirs.status.success(),
        "oracle failed: {}",
        String::from_utf8_lossy(&theirs.stderr)
    );
    assert!(
        mine.status.success(),
        "mm failed: {}",
        String::from_utf8_lossy(&mine.stderr)
    );
    if let Some((i, (x, y))) = m
        .lines()
        .zip(t.lines())
        .enumerate()
        .find(|(_, (x, y))| x != y)
    {
        panic!("line {i} differs:\n  mm:     {x}\n  python: {y}");
    }
    assert_eq!(
        m.lines().count(),
        t.lines().count(),
        "output lengths differ"
    );
    assert!(m.lines().count() > 2000, "too little output");
}
