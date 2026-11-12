//! Differential test: the own parser against the phase-1 ruff front end.
//!
//! Run with `cargo test -p mm-parse --features ruff-oracle --test
//! ruff_parity` (or `cargo xtask parse-diff`). Every corpus file, own case,
//! bench program and the prelude, plus the snippets below, must give an
//! identical AST, or the identical error when the old lowering rejected it.
//! Where ruff itself reports a syntax error, both must fail with a
//! `SyntaxError` (subclass); the own parser's message follows `CPython` 3.14
//! rather than ruff.

#![cfg(feature = "ruff-oracle")]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::doc_markdown,
    reason = "test code"
)]

use std::{fmt::Write as _, fs, path::PathBuf};

use mm_compile::CompileError;
use mm_compile::ast::Module;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in ["conformance/monty", "conformance/own", "bench"] {
        for e in fs::read_dir(root().join(dir)).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "py") {
                files.push(p);
            }
        }
    }
    files.push(root().join("crates/mm-engine/src/prelude.py"));
    let stdlib = root().join("crates/mm-stdlib/python");
    if let Ok(rd) = fs::read_dir(stdlib) {
        for e in rd {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "py") {
                files.push(p);
            }
        }
    }
    files.sort();
    files
}

/// Snippets for constructs the corpus covers thinly.
const SNIPPETS: &[&str] = &[
    "x = 1,\n",
    "x = (1,)\ny = ()\nz = (1)\n",
    "a, = b\n[a, *b] = c\n(a), b = c\n",
    "f(x for x in y)\nf((x for x in y))\nf(a, *b, c=1, **d)\nf(*a, *b, k=1, *c)\n",
    "x[1:2, ::3, :]\nx[:]\nx[1:]\nx[::]\nx[a:=1]\nx[*a]\nx[*a, b]\nx[1,]\n",
    "x = -5\ny = --5\nz = -(5)\nw = -9223372036854775808\nv = -0.0\nu = -1e400\n",
    "x = 0x_ff + 0o17 + 0b1_0 + 1_000 + 99999999999999999999 + 0xffffffffffffffffff\n",
    "x = 18446744073709551615\ny = 9223372036854775808\nz = 1_8446744073709551616\n",
    "x = 1.5 + .5 + 5. + 1e5 + 1_0.0_1 + 1E-3\n",
    "s = 'a' \"b\" '''c''' r'\\d' u'e'\nb = b'x' rb'\\y' Br'z'\n",
    "s = 'tab\\tnl\\nq\\'\\\"\\\\ \\x41\\101\\u00e9\\U0001F600\\q\\\n'\n",
    "b = b'\\x41\\101\\777\\q\\u1234'\n",
    "f'{x}' f\"{y!r:>{w}}\" 'lit'\nf'{x=}' f'{x = }' f'{x=!s}' f'{x=:>4}' f'{x=:}'\n",
    "f'{{}}' f'{x!a}' f'{\"nested\"}' f'{f\"{y}\"}' f'''{\n1\n}'''\n",
    "f'{a:{b:{c}}}'\nf'{a}{b}' f'' ''\nf'\\{x}' rf'\\{x}' f'a\\nb{x}\\tc'\n",
    "x = f'{x:%H:%M}' + f'{x!r:^10}' + f'{x,}' + f'{x, y}' + f'{*a,}'\n",
    "lambda: 0\nlambda x, /, y=1, *a, z, w=2, **k: x\nlambda *, k: k\n",
    "def f(a, /, b: int = 1, *args: str, c, d=2, **kw: int) -> None: pass\n",
    "def f(*, a): pass\ndef g(a, b, /): pass\nasync def h(): await x\n",
    "@d1\n@d2.attr(arg)\ndef f(): pass\n@d\nclass C(B, metaclass=M): pass\n",
    "@d\nasync def f(): pass\n",
    "class C:\n    x: int = 1\n    def m(self): return 1\n",
    "class C(A, *bases, k=1, **kw):\n    pass\nclass D():\n    pass\n",
    "if a:\n    pass\nelif b:\n    pass\nelif c:\n    pass\nelse:\n    pass\n",
    "if a: pass\nelse:\n    if b: pass\n",
    "while x:\n    break\nelse:\n    pass\nfor a, b in c:\n    continue\nelse:\n    pass\n",
    "for x, in y: pass\nfor (a, b) in c: pass\nfor [a, *b] in c: pass\nfor x in 1, 2: pass\n",
    "try:\n    pass\nexcept E as e:\n    pass\nexcept (A, B):\n    pass\nexcept:\n    pass\nelse:\n    pass\nfinally:\n    pass\n",
    "try:\n    pass\nexcept A, B:\n    pass\n",
    "try:\n    pass\nfinally:\n    pass\n",
    "with a as b, c as (d, e), f: pass\nwith (a as b, c): pass\nwith (a, b) as c: pass\nwith (a): pass\nwith (a, b): pass\nwith (yield): pass\n",
    "with (a for a in b): pass\nwith (a)(b) as c: pass\nwith (a, b,): pass\n",
    "global a, b\nnonlocal c\ndel a, b[1], c.d\ndel (a, b)\ndel [a]\nassert x, 'm'\nraise\nraise E from c\n",
    "import a.b.c as d, e\nfrom . import x\nfrom ..a.b import (c as d, e,)\nfrom ... import z\nfrom .... import w\n",
    "from __future__ import annotations, division\n",
    "x = yield\ny = yield a, b\nz = yield from g\nawait w\n(yield)\n",
    "x = a if b else c if d else e\nx = not a and b or not c\nx = a < b <= c != d in e not in f is g is not h\n",
    "x = a | b ^ c & d << e >> f + g - h * i / j // k % l @ m ** -n ** o\n",
    "x = ~a ** b\ny = -a ** -b\nz = (a + b) * c\nw = a.b.c[d](e)(f)[g]\n",
    "x = [1, 2, *a]\ny = {1, *b}\nz = {1: 2, **c, 3: 4}\nw = {**d}\nv = [i for i in x if i if j for k in i]\n",
    "x = {k: v for k, v in d.items()}\ny = {s for s in t}\nz = (n for n in m)\n",
    "if (n := 10) > 5: pass\nprint(n := 1)\n[y := f(x), y**2]\n",
    "x = ...\ny = None, True, False\n",
    "x: int\ny: 'str' = 'a'\n(z): int = 1\na.b: int\nc[0]: int = 2\n",
    "x += 1\nx -= 1\nx *= 1\nx /= 1\nx //= 1\nx %= 1\nx **= 1\nx <<= 1\nx >>= 1\nx &= 1\nx |= 1\nx ^= 1\nx @= 1\n",
    "a = b = c = 1\nx = y = f()\nreturn_value = f(x)\n",
    "def f():\n    return g(x)\ndef h():\n    return x.g(y)\n",
    "x = 1; y = 2;\nif x: a = 1; b = 2\n",
    "x = (1 +\n     2)\ny = [\n  1,\n  2,\n]\nz = 1 + \\\n    2\n",
    "if x:\n\tpass\n",
    "x = 1\r\ny = 2\r\nif x:\r\n    z = '''a\r\nb'''\r\n",
    "\u{feff}x = 1\n",
    "\u{e9}t\u{e9} = 1\nprint(\u{e9}t\u{e9})\n",
    "x = '\u{e9}\u{1F600}'\ny = f'\u{e9}{x}\u{1F600}'\n",
    "x = 1 # comment\n# only comment\n\n   \n  # indented comment\ny = 2\n",
    "def f():\n    x = 1\n\n    # c\n    return x\n",
    "print(1, 2, sep='', end='')\nf(a)(b)(c)\n",
    "a = (\n    1  # c\n    + 2\n)\n",
    "x = [1,\n# c\n2]\n",
    "match x:\n    case 1:\n        pass\n    case [a, *b] if a:\n        pass\n    case {'k': v, **r}:\n        pass\n    case P(a, b=c) | Q() as z:\n        pass\n    case _:\n        pass\n",
    "match = 1\nmatch.x = 2\nmatch(x)\nmatch[1] = 3\n",
    "type X = int\ntype = 1\ntype(x)\n",
    "def f[T](x): pass\n",
    "class C[T]: pass\n",
    "x = 1j\n",
    "x = t'hi {x}'\n",
    "async def f():\n    async for x in y: pass\n",
    "async def f():\n    async with a: pass\n",
    "async def f():\n    [x async for x in y]\n",
    "try:\n    pass\nexcept* E:\n    pass\n",
    "from x import *\n",
    "def f():\n    from x import *\n",
    "from __future__ import teleport\n",
    "lazy import x\n",
    "class C(metaclass=M): pass\n",
    "class C:\n    __slots__ = ()\n",
    "class C:\n    def __del__(self): pass\n",
    "class C:\n    def __getattribute__(self, n): pass\n",
    "x = {**a for a in b}\n",
    "if a:\n    x = 1j\nelif b:\n    y = 2j\nelse:\n    z = 3j\n",
    "x = f(1j, t'a')\ny = 1j\n",
    "x = [1j for a in b]\n",
    "assert 1j, 2j\n",
    "x = 1j = 2j\n",
    "with a as (b, c): pass\n",
    "for x in y:\n    pass\nelse:\n    x = 1j\n",
    "class C(1j, metaclass=M):\n    __slots__ = 2j\n",
    "@d(1j)\ndef f[T](): pass\n",
    "x = (\n  1 +\n  2\n) * 3\nf(\n  a\n)[\n  b\n]\n",
    "x = a.b(c) + d[e] - f\n",
    "x = (a) + (b)\ny = (a)(b)\nz = (a)[b]\n",
    "x = a + b # c\nx = a \\\n + b\n",
    "x = a+b\nx = a**b\nx = a//b\nx = a<<b\n",
    "x = (a\n  # c\n  + b)\n",
    "x = '\u{e9}' + y\nz = f(\u{e9}t\u{e9})[0]\n",
    "f(**{'a': 1})\nf(*[1], **{})\n",
    "print(*args, sep='')\n",
    "x = [*a, *b]\n*a, b = c\n",
    "x = *a, *b\n",
    "def f():\n    yield\n    yield x\n    x = yield\n    await y\n",
    "x = (yield)\n",
    "s = 'a\\\nb'\n",
    "s = '''a\nb\\\nc'''\n",
    "x = b'\\N{BULLET}'\n",
    "x = f'{x!r}' f'{y}'\n",
    "x = '' f'{a}' ''\n",
    "x = 0\ny = 00\nz = 0_0\n",
    "x = 1 if 2 else 3\n",
    "x = 1if 2else 3\n",
    "x = [1for x in y]\n",
    "x = 0x1for\n",
    "x = a[b][c]\n",
    "x = {}\ny = []\nz = ()\n",
    "x = ((a, b), [c, d])\n",
    "f(a,)\nf(a, b,)\nf(*a,)\nf(**a,)\n",
    "def f(a,): pass\ndef g(*a,): pass\ndef h(**a,): pass\nlambda a,: 0\n",
    "@property\ndef f(self): pass\n",
    "class A:\n    @staticmethod\n    def m(): pass\n",
    "if True:\n    pass\n# trailing comment\n",
    "def f():\n    pass\n\n\n\n",
    "x = 1\n\\\n\ny = 2\n",
    "x = 1;\n",
    "a = 1\n  # comment at other indent\nb = 2\n",
    "if x:\n    a\n    # c\n  # d\n    b\n",
    "x = '''\n'''\n",
    "x = (  # c\n  1)\n",
    "x = {\n  'a': 1,\n  'b': 2,\n}\n",
    "while (x := f()) is not None: pass\n",
    "x = [y for y in (1, 2)]\n",
    "x = a if (b) else (c)\n",
    "x = not (a)\n",
    "x = -(a)\n",
    "x = (-a)\n",
    "x = (not a)\n",
    "x = f(a)(b)\n",
    "x = (a.b).c\n",
    "x = (a)[b][c]\n",
    "x = (f)(a)\n",
    "x = ((f))(a)\n",
    "x = f(a\n  )\n",
    "x = (a,\n  b)\n",
    "x = a[\n1]\n",
    "x = (a\n)\n",
    "print(f'{x}' 'y' f'{z}')\n",
    "print(f'''{\nx\n}''')\n",
    "x = f'{x:{\"a\"}}'\n",
    "x = f'{x}' \\\n    f'{y}'\n",
    "x = f'{ x }'\n",
    "x = f'{x\n}'\n",
    "x = f'{x # c\n}'\n",
    "x = f'{x:{y}d}'\n",
    "x = f'{x!r:{y}}'\n",
    "x = f'{x:{y}.{z}f}'\n",
    "x = f'{x:\\n}'\n",
    "x = f'{x:{{}}}'\n",
    // Class-body annotations, stored as strings.
    "class C:\n    'doc'\n    a: int\n    (b): int = 1\n    c.d: int\n    if x:\n        e: Dict[str, 'C'] = {}\n    def m(self):\n        f: int = 1\n    class D:\n        g: -1 ** 2 | None\n    h: u'x' + b'\\n' + f'{y!r:>{w}}'\n",
    "class C:\n    a: 1.50\n    b: lambda: 0\n    c: [x for x in y]\n    d: List[1j]\n    e: (yield)\n    f: x if y else z\n    g: not a and b or c < d\n",
    "class C:\n    def m(self): pass\nclass E:\n    class F:\n        x: int\n",
    // Lines ending in a lone `\r`, `\r\n` and `\n`, mixed: positions count
    // each as one line break, in code, comments, continuations and strings.
    "x = 1\ry = 2\r\nz = (3 +\r 4)\n# c\rw = a + \\\r  b\rif x:\r    s = '''a\rb'''\r\n    t = f'{x}\\\r'\r",
];

/// Syntax-error snippets: both front ends must reject them.
const BAD: &[&str] = &[
    "x = = 1\n",
    "x = (1,\n",
    "x = )\n",
    "x = (1]\n",
    "if x\n  pass\n",
    "if x:\npass\n",
    "  x = 1\n",
    "if x:\n    a\n  b\n",
    "s = 'abc\n",
    "s = '''abc\n",
    "s = f'abc\n",
    "x = 1 +\n",
    "f() = 1\n",
    "1 = x\n",
    "for 1 in x: pass\n",
    "del 1\n",
    "x + 1 += 1\n",
    "f(**a, *b)\n",
    "f(a=1, b)\n",
    "def f(a=1, b): pass\n",
    "f(x for x in y, 1)\n",
    "x = 0777\n",
    "x = 1_\n",
    "x = 0x\n",
    "x = $\n",
    "f'{'\n",
    "f'{}'\n",
    "f'}'\n",
    "f'{x!z}'\n",
    "lambda: yield\n",
    "a, b += 1\n",
    "from x import a,\n",
    "def f(*): pass\n",
    "def f(**k, a): pass\n",
    "try:\n  pass\n",
    "x = 1 if 2\n",
    "a = b = \n",
    "x = {**a: 1}\n",
    "x = 'a' b'c'\n",
    "x = b'\u{e9}'\n",
    "x = '\\x4'\n",
    "a b\n",
    "f(a b)\n",
    "if x = 1: pass\n",
    "class\n",
    "def f(:\n",
    "import\n",
    "x = 1; ; y = 2\n",
    "else: pass\n",
    "x[1:2:3:4]\n",
    "x = {1: 2, 3}\n",
    "x = 1\n\\",
    "x = (a b)\n",
    "x = [a b]\n",
    "f(a.b=1)\n",
    "(a, b): int = 1\n",
    "x = yield = 1\n",
    "with a as 1: pass\n",
    "x = a if b else\n",
    "(*a)\n",
    "x = 1 2\n",
    "f'{x!}'\n",
    "f'{lambda x: 1}'\n",
    "async x = 1\n",
    "if True:\n    pass\n  else:\n    pass\n",
    "x = 0b2\n",
    "x = 1e\n",
    "x = 1.e\n",
    "x = 1jj\n",
    "\u{a0}x = 1\n",
    "x = a\u{20ac}\n",
    "t'x' 'y'\n",
    "def f(a, a=1, /, b): pass\n",
    "x = [i for i in *a]\n",
    "f(*)\n",
    "x = (yield) = 1\n",
    "del *a\n",
    "x = not\n",
    "nonlocal\n",
    "for x in y\n  pass\n",
    "class A(:\n",
    "x = '\\U00110000'\n",
];

fn same_error(a: &CompileError, b: &CompileError) -> bool {
    a.kind == b.kind && a.msg == b.msg && a.pos == b.pos
}

fn first_diff(a: &Module, b: &Module) -> String {
    let da = format!("{a:#?}");
    let db = format!("{b:#?}");
    let la: Vec<&str> = da.lines().collect();
    let lb: Vec<&str> = db.lines().collect();
    let n = la.iter().zip(&lb).take_while(|(x, y)| x == y).count();
    let mut out = String::new();
    let from = n.saturating_sub(12);
    for (i, l) in la.iter().enumerate().take(n + 3).skip(from) {
        let _ = writeln!(out, "{} own  {l}", if i == n { ">" } else { " " });
    }
    for (i, l) in lb.iter().enumerate().take(n + 3).skip(n) {
        let _ = writeln!(out, "{} ruff {l}", if i == n { ">" } else { " " });
    }
    out
}

/// Compares the two front ends on one source; `Err` describes a mismatch.
fn compare(src: &str) -> Result<(), String> {
    let own = mm_parse::parse(src);
    let ruff = mm_parse::ruff::parse(src);
    match (&own, &ruff) {
        (Ok(a), Ok(b)) if a == b => Ok(()),
        (Ok(a), Ok(b)) => Err(first_diff(a, b)),
        (Err(a), Err(b)) => {
            let ruff_syntax = ruff_python_parser::parse_module(src).is_err()
                && !b.msg.starts_with("too many")
                && b.kind != "RecursionError";
            if ruff_syntax {
                if matches!(a.kind, "SyntaxError" | "IndentationError" | "TabError") {
                    Ok(())
                } else {
                    Err(format!("own {a:?}\nruff syntax error {b:?}"))
                }
            } else if same_error(a, b) {
                Ok(())
            } else {
                Err(format!("own  {a:?}\nruff {b:?}"))
            }
        }
        (Ok(_), Err(b)) => Err(format!("own accepts; ruff {b:?}")),
        (Err(a), Ok(_)) => Err(format!("own {a:?}; ruff accepts")),
    }
}

#[test]
fn corpus_parses_identically() {
    let files = corpus();
    let mut bad = Vec::new();
    for f in &files {
        let src = fs::read_to_string(f).unwrap();
        if let Err(d) = compare(&src) {
            bad.push(format!("{}:\n{d}", f.display()));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} files differ:\n{}",
        bad.len(),
        files.len(),
        bad.join("\n")
    );
    eprintln!("{} files parse identically", files.len());
}

#[test]
fn snippets_parse_identically() {
    let mut bad = Vec::new();
    for s in SNIPPETS {
        if let Err(d) = compare(s) {
            bad.push(format!("{s:?}:\n{d}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} snippets differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn syntax_errors_are_rejected_by_both() {
    let mut bad = Vec::new();
    for s in BAD {
        let own = mm_parse::parse(s);
        let ruff = mm_parse::ruff::parse(s);
        if own.is_ok() || ruff.is_ok() {
            bad.push(format!("{s:?}: own {own:?} ruff {:?}", ruff.map(|_| ())));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// A recorded difference from the oracle: CPython 3.14 refuses any source
/// holding a null byte ("source code cannot contain null bytes", on the
/// null byte's line). ruff accepts one in a comment or a string and
/// reports `Got unexpected token \0` elsewhere, so `compare` cannot judge
/// these; the corpus holds none.
#[test]
fn null_bytes_are_refused_where_the_oracle_differs() {
    for (s, line) in [
        ("# a\0b\nprint(1)\n", 1),
        ("x = 1\ny = \"a\0b\"\n", 2),
        ("def f(:\n  pass\n\0", 3),
    ] {
        let Err(e) = mm_parse::parse(s) else {
            panic!("{s:?} parsed");
        };
        assert_eq!((e.msg.as_str(), e.pos.line), (mm_parse::NULL_BYTES, line));
    }
}

/// Long flat chains are one level of nesting in both front ends, and
/// nesting around them is counted the same: the same trees, or the same
/// error, on either side of the limits. Source at the
/// nesting limit needs 3.7 MiB of stack in a debug build (docs/sdk.md),
/// more than a test thread has.
#[test]
fn chains_and_nesting_parse_identically() {
    std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(chains_and_nesting)
        .unwrap()
        .join()
        .unwrap();
}

fn chains_and_nesting() {
    let chain = |n: usize, item: &str, sep: &str| vec![item; n].join(sep);
    let mut sources = vec![
        format!("x = {}\n", chain(400, "a", " + ")),
        format!("x = {}\n", chain(300, "a", " - b * ")),
        format!("x = a{}\n", ".m(1)[i].b".repeat(150)),
        format!("x = {}z\n", "a if b else ".repeat(300)),
        format!(
            "if a:\n    pass\n{}else:\n    pass\n",
            "elif b:\n    pass\n".repeat(300)
        ),
        format!("x = f({})\n", chain(200, "a.b(c) + d", ", ")),
    ];
    for n in [190, 198, 199, 200, 201] {
        sources.push(format!(
            "x = {}{}{}\n",
            "f(".repeat(n),
            chain(50, "a", " + "),
            ")".repeat(n)
        ));
        sources.push(format!("x = {}a{}\n", "(-".repeat(n), ")".repeat(n)));
        sources.push(format!("x = {}a + b\n", "lambda: ".repeat(n)));
        sources.push(format!(
            "x = {}{}\n",
            "a if b else lambda: ".repeat(n / 2),
            "c.d.e"
        ));
        sources.push(format!("x = {}a\n", "not ".repeat(n)));
    }
    let mut bad = Vec::new();
    for s in &sources {
        // The two front ends place a nesting RecursionError at different
        // tokens (they count depth at different nodes); what must agree is
        // whether the source compiles and which error it raises.
        let too_deep = |r: Result<Module, CompileError>| match r {
            Err(e) if e.kind == "RecursionError" => Some(e.msg),
            _ => None,
        };
        let own = too_deep(mm_parse::parse(s));
        if own.is_some() && own == too_deep(mm_parse::ruff::parse(s)) {
            continue;
        }
        if let Err(d) = compare(s) {
            bad.push(format!("{}...:\n{d}", &s[..s.len().min(60)]));
        }
    }
    assert!(bad.is_empty(), "{} differ:\n{}", bad.len(), bad.join("\n"));
}
