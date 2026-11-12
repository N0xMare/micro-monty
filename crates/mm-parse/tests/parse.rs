//! The own parser without the ruff oracle: syntax errors as `CPython`
//! 3.14.7 reports them, nesting limits without stack overflow, and the
//! constructs micro-monty rejects.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::doc_markdown,
    reason = "test code"
)]

use mm_compile::ast::{Const, ExprKind, FPart, StmtKind};

/// (source, exception type, message, line, 1-based column) as CPython
/// 3.14.7 reports them (`compile(source, "<s>", "exec")`).
const CPYTHON: &[(&str, &str, &str, u32, u32)] = &[
    ("x = = 1", "SyntaxError", "invalid syntax", 1, 5),
    ("x = (1,", "SyntaxError", "'(' was never closed", 1, 5),
    ("x = )", "SyntaxError", "unmatched ')'", 1, 5),
    (
        "x = (1]",
        "SyntaxError",
        "closing parenthesis ']' does not match opening parenthesis '('",
        1,
        7,
    ),
    (
        "x = (1,\n2]",
        "SyntaxError",
        "closing parenthesis ']' does not match opening parenthesis '(' on line 1",
        2,
        2,
    ),
    ("if x\n  pass", "SyntaxError", "expected ':'", 1, 5),
    (
        "if x:\npass",
        "IndentationError",
        "expected an indented block after 'if' statement on line 1",
        2,
        1,
    ),
    ("  x = 1", "IndentationError", "unexpected indent", 1, 2),
    (
        "if x:\n    a\n  b",
        "IndentationError",
        "unindent does not match any outer indentation level",
        3,
        4,
    ),
    (
        "if x:\n\ta\n        b",
        "TabError",
        "inconsistent use of tabs and spaces in indentation",
        3,
        1,
    ),
    (
        "s = 'abc",
        "SyntaxError",
        "unterminated string literal (detected at line 1)",
        1,
        5,
    ),
    (
        "s = '''abc",
        "SyntaxError",
        "unterminated triple-quoted string literal (detected at line 1)",
        1,
        5,
    ),
    (
        "s = f'abc",
        "SyntaxError",
        "unterminated f-string literal (detected at line 1)",
        1,
        5,
    ),
    ("x = 1 +", "SyntaxError", "invalid syntax", 1, 8),
    (
        "f() = 1",
        "SyntaxError",
        "cannot assign to function call here. Maybe you meant '==' instead of '='?",
        1,
        1,
    ),
    (
        "1 = x",
        "SyntaxError",
        "cannot assign to literal here. Maybe you meant '==' instead of '='?",
        1,
        1,
    ),
    (
        "for 1 in x: pass",
        "SyntaxError",
        "cannot assign to literal",
        1,
        5,
    ),
    ("del 1", "SyntaxError", "cannot delete literal", 1, 5),
    (
        "def f(*a=1): pass",
        "SyntaxError",
        "var-positional argument cannot have default value",
        1,
        9,
    ),
    (
        "def f(**a=1): pass",
        "SyntaxError",
        "var-keyword argument cannot have default value",
        1,
        10,
    ),
    (
        "try:\n    pass\nexcept* A:\n    pass\nexcept B:\n    pass",
        "SyntaxError",
        "cannot have both 'except' and 'except*' on the same 'try'",
        5,
        1,
    ),
    (
        "if x = 1: pass",
        "SyntaxError",
        "invalid syntax. Maybe you meant '==' or ':=' instead of '='?",
        1,
        4,
    ),
    (
        "if f() = 1: pass",
        "SyntaxError",
        "cannot assign to function call here. Maybe you meant '==' instead of '='?",
        1,
        4,
    ),
    (
        "f(a + b = 1)",
        "SyntaxError",
        "expression cannot contain assignment, perhaps you meant \"==\"?",
        1,
        3,
    ),
    (
        "{1:}",
        "SyntaxError",
        "expression expected after dictionary key and ':'",
        1,
        3,
    ),
    (
        "{1:, 2: 3}",
        "SyntaxError",
        "expression expected after dictionary key and ':'",
        1,
        3,
    ),
    (
        "match x:\npass",
        "IndentationError",
        "expected an indented block after 'match' statement on line 1",
        2,
        1,
    ),
    (
        "f(**{}, *a)",
        "SyntaxError",
        "iterable argument unpacking follows keyword argument unpacking",
        1,
        7,
    ),
    (
        "del f()",
        "SyntaxError",
        "cannot delete function call",
        1,
        5,
    ),
    (
        "x + 1 += 1",
        "SyntaxError",
        "'expression' is an illegal expression for augmented assignment",
        1,
        1,
    ),
    (
        "f(**a, *b)",
        "SyntaxError",
        "iterable argument unpacking follows keyword argument unpacking",
        1,
        6,
    ),
    (
        "f(a=1, b)",
        "SyntaxError",
        "positional argument follows keyword argument",
        1,
        9,
    ),
    (
        "f(**a, b)",
        "SyntaxError",
        "positional argument follows keyword argument unpacking",
        1,
        9,
    ),
    (
        "def f(a=1, b): pass",
        "SyntaxError",
        "parameter without a default follows parameter with a default",
        1,
        12,
    ),
    (
        "f(x for x in y, 1)",
        "SyntaxError",
        "Generator expression must be parenthesized",
        1,
        3,
    ),
    (
        "x = 0777",
        "SyntaxError",
        "leading zeros in decimal integer literals are not permitted; use an 0o prefix for octal integers",
        1,
        5,
    ),
    ("x = 1_", "SyntaxError", "invalid decimal literal", 1, 6),
    ("x = 0x", "SyntaxError", "invalid hexadecimal literal", 1, 6),
    ("x = 1__0", "SyntaxError", "invalid decimal literal", 1, 6),
    (
        "x = 09",
        "SyntaxError",
        "leading zeros in decimal integer literals are not permitted; use an 0o prefix for octal integers",
        1,
        5,
    ),
    (
        "x = 0o8",
        "SyntaxError",
        "invalid digit '8' in octal literal",
        1,
        7,
    ),
    ("x = 1e", "SyntaxError", "invalid decimal literal", 1, 5),
    ("x = $", "SyntaxError", "invalid syntax", 1, 5),
    (
        "x = a\u{20ac}",
        "SyntaxError",
        "invalid character '\u{20ac}' (U+20AC)",
        1,
        6,
    ),
    (
        "x = \u{a0}",
        "SyntaxError",
        "invalid non-printable character U+00A0",
        1,
        5,
    ),
    (
        "x = 1 \\ 2",
        "SyntaxError",
        "unexpected character after line continuation character",
        1,
        8,
    ),
    ("f'{'", "SyntaxError", "f-string: expecting '}'", 1, 4),
    (
        "f'{}'",
        "SyntaxError",
        "f-string: valid expression required before '}'",
        1,
        4,
    ),
    (
        "f'}'",
        "SyntaxError",
        "f-string: single '}' is not allowed",
        1,
        3,
    ),
    (
        "f'{x!z}'",
        "SyntaxError",
        "f-string: invalid conversion character 'z': expected 's', 'r', or 'a'",
        1,
        6,
    ),
    (
        "f'{x!}'",
        "SyntaxError",
        "f-string: missing conversion character",
        1,
        6,
    ),
    ("f'{x'", "SyntaxError", "f-string: expecting '}'", 1, 5),
    (
        "x = yield = 1",
        "SyntaxError",
        "assignment to yield expression not possible",
        1,
        5,
    ),
    (
        "a, b += 1",
        "SyntaxError",
        "'tuple' is an illegal expression for augmented assignment",
        1,
        1,
    ),
    (
        "(a, b): int = 1",
        "SyntaxError",
        "only single target (not tuple) can be annotated",
        1,
        1,
    ),
    (
        "import a.b as c.d",
        "SyntaxError",
        "cannot use attribute as import target",
        1,
        15,
    ),
    (
        "from x import a,",
        "SyntaxError",
        "trailing comma not allowed without surrounding parentheses",
        1,
        17,
    ),
    (
        "def f(*): pass",
        "SyntaxError",
        "named arguments must follow bare *",
        1,
        7,
    ),
    (
        "def f(*, **k): pass",
        "SyntaxError",
        "named arguments must follow bare *",
        1,
        7,
    ),
    (
        "def f(**k, a): pass",
        "SyntaxError",
        "arguments cannot follow var-keyword argument",
        1,
        12,
    ),
    (
        "class A(x for x in y): pass",
        "SyntaxError",
        "invalid syntax",
        1,
        11,
    ),
    (
        "x = [i for i in *a]",
        "SyntaxError",
        "invalid syntax",
        1,
        17,
    ),
    (
        "try:\n  pass\n",
        "SyntaxError",
        "expected 'except' or 'finally' block",
        2,
        7,
    ),
    (
        "try:\n  pass\nx = 1",
        "SyntaxError",
        "expected 'except' or 'finally' block",
        3,
        1,
    ),
    (
        "while x:\npass",
        "IndentationError",
        "expected an indented block after 'while' statement on line 1",
        2,
        1,
    ),
    (
        "print 'x'",
        "SyntaxError",
        "Missing parentheses in call to 'print'. Did you mean print(...)?",
        1,
        1,
    ),
    (
        "x = 1 if 2",
        "SyntaxError",
        "expected 'else' after 'if' expression",
        1,
        5,
    ),
    ("a = b = ", "SyntaxError", "invalid syntax", 1, 9),
    ("@\ndef f(): pass", "SyntaxError", "invalid syntax", 1, 2),
    ("x = {**a: 1}", "SyntaxError", "invalid syntax", 1, 9),
    ("x: int: int", "SyntaxError", "invalid syntax", 1, 7),
    ("async x = 1", "SyntaxError", "invalid syntax", 1, 7),
    (
        "x = 'a' b'c'",
        "SyntaxError",
        "cannot mix bytes and nonbytes literals",
        1,
        13,
    ),
    (
        "x = b'\u{e9}'",
        "SyntaxError",
        "bytes can only contain ASCII literal characters",
        1,
        5,
    ),
    (
        "x = '\\x4'",
        "SyntaxError",
        "(unicode error) 'unicodeescape' codec can't decode bytes in position 0-2: truncated \\xXX escape",
        1,
        5,
    ),
    (
        "def f():\n  x = 1\n   y = 2",
        "IndentationError",
        "unexpected indent",
        3,
        3,
    ),
    (
        "if True:\n    pass\n  else:\n    pass",
        "IndentationError",
        "unindent does not match any outer indentation level",
        3,
        8,
    ),
    ("x = (", "SyntaxError", "'(' was never closed", 1, 5),
    ("x = [1, 2", "SyntaxError", "'[' was never closed", 1, 5),
    ("x = {", "SyntaxError", "'{' was never closed", 1, 5),
    ("a b", "SyntaxError", "invalid syntax", 1, 3),
    ("a = 1 2", "SyntaxError", "invalid syntax", 1, 7),
    (
        "f(a b)",
        "SyntaxError",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        3,
    ),
    // A stray `\` the parser never fetched (CPython's deferred
    // E_LINECONT) leaves the parser's error standing.
    (
        "f(x=fn a|b, y=\\c)",
        "SyntaxError",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        5,
    ),
    ("f(x=^a \\c)", "SyntaxError", "invalid syntax", 1, 5),
    (
        "f(a b \\c)",
        "SyntaxError",
        "unexpected character after line continuation character",
        1,
        8,
    ),
    ("f(a b\nc \\d)", "SyntaxError", "'(' was never closed", 1, 2),
    (
        "if x = 1: pass",
        "SyntaxError",
        "invalid syntax. Maybe you meant '==' or ':=' instead of '='?",
        1,
        4,
    ),
    ("x = not", "SyntaxError", "invalid syntax", 1, 8),
    ("class", "SyntaxError", "invalid syntax", 1, 6),
    ("def f(:", "SyntaxError", "invalid syntax", 1, 7),
    (
        "for x in range(10)\n  pass",
        "SyntaxError",
        "expected ':'",
        1,
        19,
    ),
    (
        "import",
        "SyntaxError",
        "Expected one or more names after 'import'",
        1,
        7,
    ),
    ("x = ...a", "SyntaxError", "invalid syntax", 1, 8),
    ("a.1", "SyntaxError", "invalid syntax", 1, 2),
    (
        "x = 5 if 3 else",
        "SyntaxError",
        "expected expression after 'else', but statement is given",
        1,
        16,
    ),
    ("x = 1; ; y = 2", "SyntaxError", "invalid syntax", 1, 8),
    ("else: pass", "SyntaxError", "invalid syntax", 1, 1),
    ("elif x: pass", "SyntaxError", "invalid syntax", 1, 1),
    ("x = lambda: yield", "SyntaxError", "invalid syntax", 1, 13),
    ("print(*)", "SyntaxError", "Invalid star expression", 1, 8),
    (
        "x = (*a)",
        "SyntaxError",
        "cannot use starred expression here",
        1,
        6,
    ),
    (
        "f(a=)",
        "SyntaxError",
        "expected argument value expression",
        1,
        3,
    ),
    ("x[1:2:3:4]", "SyntaxError", "invalid syntax", 1, 8),
    (
        "x = {1: 2, 3}",
        "SyntaxError",
        "':' expected after dictionary key",
        1,
        12,
    ),
    ("x = {1, 2: 3}", "SyntaxError", "invalid syntax", 1, 10),
    (
        "x = 0b2",
        "SyntaxError",
        "invalid digit '2' in binary literal",
        1,
        7,
    ),
    ("x = 1.e", "SyntaxError", "invalid decimal literal", 1, 6),
    ("x = 1.0_", "SyntaxError", "invalid decimal literal", 1, 8),
    ("x = 1j_", "SyntaxError", "invalid imaginary literal", 1, 6),
    (
        "t'x' 'y'",
        "SyntaxError",
        "cannot mix t-string literals with string or bytes literals",
        1,
        1,
    ),
    (
        "x = f'{x:\n}'",
        "SyntaxError",
        "f-string: newlines are not allowed in format specifiers for single quoted f-strings",
        1,
        10,
    ),
    (
        "x = 1\n\\",
        "SyntaxError",
        "unexpected EOF while parsing",
        2,
        2,
    ),
    (
        "if 1:\n  x\n\ty",
        "TabError",
        "inconsistent use of tabs and spaces in indentation",
        3,
        1,
    ),
    ("x = 1 ! 2", "SyntaxError", "invalid syntax", 1, 7),
    (
        "f'{x! r}'",
        "SyntaxError",
        "f-string: conversion type must come right after the exclamation mark",
        1,
        5,
    ),
    (
        "u'x' b'y'",
        "SyntaxError",
        "cannot mix bytes and nonbytes literals",
        1,
        10,
    ),
    (
        "f'{x:{y:{z:{w}}}}'",
        "SyntaxError",
        "f-string: expressions nested too deeply",
        1,
        11,
    ),
    (
        "f'{lambda x:1}'",
        "SyntaxError",
        "f-string: lambda expressions are not allowed without parentheses",
        1,
        4,
    ),
    (
        "f'{x}}'",
        "SyntaxError",
        "f-string: single '}' is not allowed",
        1,
        6,
    ),
    (
        "f'{{}'",
        "SyntaxError",
        "f-string: single '}' is not allowed",
        1,
        5,
    ),
    (
        "f'{ }'",
        "SyntaxError",
        "f-string: valid expression required before '}'",
        1,
        5,
    ),
    (
        "f'{x!r=}'",
        "SyntaxError",
        "f-string: expecting ':' or '}'",
        1,
        7,
    ),
    ("x = `1`", "SyntaxError", "invalid syntax", 1, 5),
    (
        "def f():\n  return\n x = 1",
        "IndentationError",
        "unindent does not match any outer indentation level",
        3,
        7,
    ),
    (
        "if x:\n",
        "IndentationError",
        "expected an indented block after 'if' statement on line 1",
        1,
        6,
    ),
    (
        "class A:\n",
        "IndentationError",
        "expected an indented block after class definition on line 1",
        1,
        9,
    ),
    (
        "def f():\n",
        "IndentationError",
        "expected an indented block after function definition on line 1",
        1,
        9,
    ),
    ("x = [1,\n2", "SyntaxError", "'[' was never closed", 1, 5),
    (
        "x = 'a' 'b",
        "SyntaxError",
        "unterminated string literal (detected at line 1)",
        1,
        9,
    ),
    ("(1, 2", "SyntaxError", "'(' was never closed", 1, 1),
    ("{'a': 1", "SyntaxError", "'{' was never closed", 1, 1),
    (
        "print(1\nprint(2)",
        "SyntaxError",
        "'(' was never closed",
        1,
        6,
    ),
    (
        "x = 1\n  y = 2",
        "IndentationError",
        "unexpected indent",
        2,
        2,
    ),
    ("while True\n  pass", "SyntaxError", "expected ':'", 1, 11),
    (
        "f(a, b c)",
        "SyntaxError",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        6,
    ),
    (
        "[a, b c]",
        "SyntaxError",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        5,
    ),
    ("x = 1 +* 2", "SyntaxError", "invalid syntax", 1, 8),
    ("x ++", "SyntaxError", "invalid syntax", 1, 5),
    ("def f(x y): pass", "SyntaxError", "invalid syntax", 1, 9),
    ("import a.", "SyntaxError", "invalid syntax", 1, 10),
    (
        "from . import",
        "SyntaxError",
        "Expected one or more names after 'import'",
        1,
        14,
    ),
    (
        "with open(f) as: pass",
        "SyntaxError",
        "invalid syntax",
        1,
        16,
    ),
    ("for in x: pass", "SyntaxError", "invalid syntax", 1, 5),
    ("x = (1 for)", "SyntaxError", "invalid syntax", 1, 11),
    ("@dec\nx = 1", "SyntaxError", "invalid syntax", 2, 1),
    ("return x y", "SyntaxError", "invalid syntax", 1, 10),
    (
        "try:\n  x\nexcept E e:\n  pass",
        "SyntaxError",
        "invalid syntax",
        3,
        10,
    ),
    ("x = [*]", "SyntaxError", "Invalid star expression", 1, 7),
    ("a.b.c = = 1", "SyntaxError", "invalid syntax", 1, 9),
    (
        "x = {1:}",
        "SyntaxError",
        "expression expected after dictionary key and ':'",
        1,
        7,
    ),
    ("x = {:1}", "SyntaxError", "invalid syntax", 1, 6),
    (
        "if x:\n  pass\nelse x:\n  pass",
        "SyntaxError",
        "expected ':'",
        3,
        6,
    ),
    ("x = 1 if else 2", "SyntaxError", "invalid syntax", 1, 10),
    ("x = not in y", "SyntaxError", "invalid syntax", 1, 9),
    ("assert", "SyntaxError", "invalid syntax", 1, 7),
    ("del", "SyntaxError", "invalid syntax", 1, 4),
    ("raise from x", "SyntaxError", "invalid syntax", 1, 7),
    ("global", "SyntaxError", "invalid syntax", 1, 7),
    ("x: = 1", "SyntaxError", "invalid syntax", 1, 4),
    (
        "class A(B C): pass",
        "SyntaxError",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        9,
    ),
    ("f(**)", "SyntaxError", "invalid syntax", 1, 5),
    ("x = 3 ** ", "SyntaxError", "invalid syntax", 1, 10),
    ("x = a[", "SyntaxError", "'[' was never closed", 1, 6),
    ("x = a[]", "SyntaxError", "invalid syntax", 1, 7),
    ("x = a.", "SyntaxError", "invalid syntax", 1, 7),
    ("x = a..b", "SyntaxError", "invalid syntax", 1, 7),
    ("1 +\n", "SyntaxError", "invalid syntax", 1, 4),
    ("pass pass", "SyntaxError", "invalid syntax", 1, 6),
    (
        "if 1: pass else: pass",
        "SyntaxError",
        "invalid syntax",
        1,
        12,
    ),
    (
        "x = '''",
        "SyntaxError",
        "unterminated triple-quoted string literal (detected at line 1)",
        1,
        5,
    ),
    (
        "x = '\\",
        "SyntaxError",
        "unterminated string literal (detected at line 1)",
        1,
        5,
    ),
    (
        "f'{x:'",
        "SyntaxError",
        "f-string: expecting '}', or format specs",
        1,
        6,
    ),
    (
        "x = f'{x=!z}'",
        "SyntaxError",
        "f-string: invalid conversion character 'z': expected 's', 'r', or 'a'",
        1,
        11,
    ),
    (
        "async def f():\n  await",
        "SyntaxError",
        "invalid syntax",
        2,
        8,
    ),
    ("x = 1e5e5", "SyntaxError", "invalid decimal literal", 1, 7),
    ("x = 5.5.5", "SyntaxError", "invalid syntax", 1, 8),
    ("x = 0o", "SyntaxError", "invalid octal literal", 1, 6),
    ("x = 0b", "SyntaxError", "invalid binary literal", 1, 6),
    (
        "x = 0xg",
        "SyntaxError",
        "invalid hexadecimal literal",
        1,
        6,
    ),
    ("x = 1__2", "SyntaxError", "invalid decimal literal", 1, 6),
    ("x = 0_x1", "SyntaxError", "invalid decimal literal", 1, 6),
    (
        "\tif x:\n\t\tpass\n        pass",
        "IndentationError",
        "unexpected indent",
        1,
        1,
    ),
    (
        "if x:\n        a\n\tb",
        "TabError",
        "inconsistent use of tabs and spaces in indentation",
        3,
        1,
    ),
    // A lone `\r` ends a line as `\n` and `\r\n` do (CPython translates
    // line endings before tokenizing), in code, comments, strings and
    // continuations.
    ("x = 1\ry = = 2\r", "SyntaxError", "invalid syntax", 2, 5),
    ("x = 1\r\ry = = 2", "SyntaxError", "invalid syntax", 3, 5),
    (
        "a = 1\n\rb = 2\r\nc = = 3",
        "SyntaxError",
        "invalid syntax",
        4,
        5,
    ),
    (
        "s = '''a\rb'''\rq = = 1",
        "SyntaxError",
        "invalid syntax",
        3,
        5,
    ),
    (
        "s = '''a\r\nb\rc\nd'''\rq = = 1",
        "SyntaxError",
        "invalid syntax",
        5,
        5,
    ),
    ("# c\rx = = 1", "SyntaxError", "invalid syntax", 2, 5),
    ("x = 1 \\\r  + = 2", "SyntaxError", "invalid syntax", 2, 5),
    (
        "s = '''a\r\n\\x1'''",
        "SyntaxError",
        "(unicode error) 'unicodeescape' codec can't decode bytes in position 2-4: truncated \\xXX escape",
        1,
        5,
    ),
    (
        "s = b'''a\r\r\\x1'''",
        "SyntaxError",
        "(value error) invalid \\x escape at position 3",
        1,
        5,
    ),
    (
        "s = '''\r\r\n\\U00110000'''",
        "SyntaxError",
        "(unicode error) 'unicodeescape' codec can't decode bytes in position 2-11: illegal Unicode character",
        1,
        5,
    ),
    (
        "x = (1 +\r\n2) = 3",
        "SyntaxError",
        "cannot assign to expression",
        1,
        6,
    ),
    ("é = 1\rxé = = 1", "SyntaxError", "invalid syntax", 2, 6),
    (
        "x = 1\r  y = 2",
        "IndentationError",
        "unexpected indent",
        2,
        2,
    ),
    (
        "if x:\rpass",
        "IndentationError",
        "expected an indented block after 'if' statement on line 1",
        2,
        1,
    ),
    (
        "if x:\r    a\r  b\r",
        "IndentationError",
        "unindent does not match any outer indentation level",
        3,
        4,
    ),
    (
        "def f():\r    return 1\r\n  x = 2\r",
        "IndentationError",
        "unindent does not match any outer indentation level",
        3,
        8,
    ),
    (
        "x = 1\ry = (\r",
        "SyntaxError",
        "'(' was never closed",
        2,
        5,
    ),
    ("f(\r1,\r2\r", "SyntaxError", "'(' was never closed", 1, 2),
    (
        "x = 1\ry = 'ab\r'",
        "SyntaxError",
        "unterminated string literal (detected at line 2)",
        2,
        5,
    ),
    (
        "x = 1\r\ny = '''ab\r",
        "SyntaxError",
        "unterminated triple-quoted string literal (detected at line 2)",
        2,
        5,
    ),
    (
        "x = 1\rf'''{a\r}'''\r\nf'{}'",
        "SyntaxError",
        "f-string: valid expression required before '}'",
        4,
        4,
    ),
    (
        "x = 1\ry = 1 \\ 2",
        "SyntaxError",
        "unexpected character after line continuation character",
        2,
        8,
    ),
];

/// A parser error against a later tokenizer error, as CPython 3.14
/// reports them (`compile(src, '<cell>', 'exec')`: message, line, offset
/// and, for a range, end offset). The first five are model-written cells
/// from real recorded sessions: an unquoted regex argument, where a stray `\\`
/// (CPython's deferred `E_LINECONT`) the parser never fetched leaves the
/// comma hint standing. Then nearby shapes, each side of every rule.
const PRECEDENCE: &[(&str, &str, usize, usize, usize)] = &[
    (
        "hits = await ctx.grep(path=\"crates/mm-engine/src/builtins.rs\", pattern=^const DEFS|n!\\(|m!\\(|fn |prelude|handoff|Cont::|Request::\")\nprint(1)\n",
        "invalid syntax",
        1,
        72,
        73,
    ),
    (
        "r = await proc.run([\"wc\", \"-l\", \"crates/mm-engine/src/builtins.rs\"])\nprint(r.output)\n# also walked() definition\nhits = await ctx.grep(path=\"crates/mm-engine/src\", pattern=fn walked|fn collect\\(|fn order\\(|fn binop\\(|fn range_sum|fn str_bytes, glob=\"*.rs\")\nfor h in hits.hits:\n    print(h)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        4,
        60,
        72,
    ),
    (
        "hits = await ctx.grep(path=\"crates/mm-engine/src\", pattern=fn need_py|fn prelude_request|fn dunder_call|fn call_request|fn call_dunder|slow = true|\\.slow = true|need_py\\()\nprint(1)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        60,
        73,
    ),
    (
        "hits = await ctx.grep(path=\"crates/mm-engine/src/intops.rs\", pattern=fn add_|BinOp::Add|burn\\(, glob=\"*.rs\")\nprint(1)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        70,
        83,
    ),
    (
        "hits = await ctx.grep(path=\"crates\", pattern=SNAP_|snapshot version|format version|MAGIC|mm-snap|\\\"MM\\\"|0x4D4D|write_snapshot\", glob=\"**/*snapshot*\")\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        46,
        75,
    ),
    (
        "ctx.grep(pattern=\\bfoo\\b)\n",
        "unexpected character after line continuation character",
        1,
        19,
        0,
    ),
    (
        "f(a b \\c)\n",
        "unexpected character after line continuation character",
        1,
        8,
        0,
    ),
    (
        "f(a b c \\d)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        3,
        6,
    ),
    (
        "f(a b, \\c)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        3,
        6,
    ),
    (
        "f(a b)\\c\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        3,
        6,
    ),
    (
        "f(a b) \\\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        3,
        6,
    ),
    (
        "x = (a b) \\ 1\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        6,
        9,
    ),
    (
        "f(x=fn a|b, y=\\c)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        5,
        11,
    ),
    ("f(x=^a \\c)\n", "invalid syntax", 1, 5, 6),
    ("x = ^a \\c\n", "invalid syntax", 1, 5, 6),
    ("f(a, ^b \\c)\n", "invalid syntax", 1, 6, 7),
    (
        "f(a=b c d \\e)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        5,
        8,
    ),
    (
        "f(*a b c \\d)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        4,
        7,
    ),
    (
        "f(**a b c \\d)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        5,
        8,
    ),
    (
        "[1 2 3 \\e]\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        2,
        5,
    ),
    (
        "{a b c \\d}\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        2,
        5,
    ),
    (
        "(a b c \\d)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        2,
        5,
    ),
    (
        "f(a b if c \\d)\n",
        "unexpected character after line continuation character",
        1,
        13,
        0,
    ),
    (
        "x = a b \\c\n",
        "unexpected character after line continuation character",
        1,
        10,
        0,
    ),
    (
        "x = a b c \\d\n",
        "unexpected character after line continuation character",
        1,
        12,
        0,
    ),
    (
        "if x b \\c: pass\n",
        "unexpected character after line continuation character",
        1,
        9,
        0,
    ),
    (
        "('s' x y \\q)\n",
        "unexpected character after line continuation character",
        1,
        11,
        0,
    ),
    (
        "(a 's' y \\q)\n",
        "unexpected character after line continuation character",
        1,
        11,
        0,
    ),
    (
        "(f's' x y \\q)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        2,
        8,
    ),
    ("f(a b\nc \\d)\n", "'(' was never closed", 1, 2, 0),
    ("x = (a b\ny = 1 \\z)\n", "'(' was never closed", 1, 5, 0),
    (
        "f(a b\n)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        3,
        6,
    ),
    (
        "f(a b c\n  \\d)\n",
        "invalid syntax. Perhaps you forgot a comma?",
        1,
        3,
        6,
    ),
    // `?`, `$` and the backtick are tokens only the parser rejects: an
    // earlier parser error stands (a real recorded cell, `x.stdout ?? x`).
    ("const x = 1; y = a ?? b\n", "invalid syntax", 1, 7, 8),
    ("x = 1\ny = a $ b\n", "invalid syntax", 2, 7, 8),
    ("import $\n", "invalid syntax", 1, 8, 9),
    (
        "f'{a $ b}'\n",
        "f-string: expecting '=', or '!', or ':', or '}'",
        1,
        6,
        7,
    ),
];

#[test]
fn tokenizer_and_parser_errors_take_cpythons_precedence() {
    let mut bad = Vec::new();
    for &(src, msg, line, col, end) in PRECEDENCE {
        let got = match mm_parse::parse(src) {
            Ok(_) => {
                bad.push(format!("{src:?}: accepted"));
                continue;
            }
            Err(e) => (
                e.msg,
                e.pos.line as usize,
                e.pos.col as usize + 1,
                if end == 0 {
                    0
                } else {
                    e.pos.end_col as usize + 1
                },
            ),
        };
        if got != (msg.to_string(), line, col, end) {
            bad.push(format!(
                "{src:?}:\n  cpython {msg} {line}:{col}-{end}\n  own     {got:?}"
            ));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn syntax_errors_match_cpython() {
    let mut bad = Vec::new();
    for &(src, kind, msg, line, col) in CPYTHON {
        match mm_parse::parse(src) {
            Ok(_) => bad.push(format!("{src:?}: accepted")),
            Err(e) => {
                if (e.kind, e.msg.as_str(), e.pos.line, e.pos.col + 1) != (kind, msg, line, col) {
                    bad.push(format!(
                        "{src:?}:\n  cpython {kind}: {msg} ({line}:{col})\n  own     {}: {} ({}:{})",
                        e.kind,
                        e.msg,
                        e.pos.line,
                        e.pos.col + 1
                    ));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Lines ending in a lone `\r` among `\n` and `\r\n`: the pre-scan's and
/// the tokenizer's limits and AST positions as CPython 3.14.7 reports them
/// (`compile`, `ast.parse`).
#[test]
fn lone_carriage_returns_end_lines_as_in_cpython() {
    let error = |src: &str| {
        let e = mm_parse::parse(src).unwrap_err();
        (e.kind, e.msg, e.pos.line, e.pos.col + 1)
    };
    for nl in ["\r", "\r\n"] {
        let src = format!("x = 1{nl}{}{}", "(".repeat(201), ")".repeat(201));
        assert_eq!(
            error(&src),
            ("SyntaxError", "too many nested parentheses".into(), 2, 201)
        );
    }
    let mut src = String::from("x = 1\r");
    for i in 0..101 {
        src += &"    ".repeat(i);
        src += "if x:\r";
    }
    src += &"    ".repeat(101);
    src += "pass\r";
    assert_eq!(
        error(&src),
        (
            "IndentationError",
            "too many levels of indentation".into(),
            102,
            1
        )
    );

    let m = mm_parse::parse("x = 1\ry = 2\r\nz = (3 +\r 4)\n").unwrap();
    let at = |p: mm_compile::ast::Pos| (p.line, p.col, p.end_line, p.end_col);
    let stmts: Vec<_> = m.body.iter().map(|s| at(s.pos)).collect();
    assert_eq!(stmts, [(1, 0, 1, 5), (2, 0, 2, 5), (3, 0, 4, 3)]);
    let StmtKind::Assign(_, value) = &m.body[2].kind else {
        panic!("{:?}", m.body[2].kind);
    };
    assert_eq!(at(value.pos), (3, 5, 4, 2));
}

/// Line breaks inside triple-quoted strings (`\r\n`, a lone `\r`) read as
/// `\n` in the value, raw, bytes and f-string text included, as CPython
/// 3.14.7's tokenizer translates them (`ast.parse`; a file, `compile` of a
/// `str` and of `bytes` agree). An escaped `\r` stays.
#[test]
fn line_breaks_in_strings_read_as_newlines_as_in_cpython() {
    fn show(parts: &[FPart]) -> String {
        parts
            .iter()
            .map(|p| match p {
                FPart::Lit(s) => format!("{s:?}"),
                FPart::Field { spec, .. } => {
                    format!("{{{}}}", spec.as_deref().map(show).unwrap_or_default())
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
    for (src, want) in [
        ("'''a\r\nb\rc\r\r\nd'''", r#""a\nb\nc\n\nd""#),
        ("r'''a\r\nb\rc'''", r#""a\nb\nc""#),
        ("b'''a\r\nb\rc'''", "[97, 10, 98, 10, 99]"),
        ("'''a\\\r\nb\\\rc'''", r#""abc""#),
        ("'''\\r\r'''", r#""\r\n""#),
        ("rb'''\\\r'''", "[92, 10]"),
        ("f'''a\r\n{x}\rb'''", r#""a\n" {} "\nb""#),
        ("f'''{x\r\n=}'''", r#""x\n=" {}"#),
        ("f'''{x=\r}'''", r#""x=\n" {}"#),
        ("f'''{x:\r\n>3}'''", r#"{"\n>3"}"#),
        ("f'''{x!r:{y}\r}'''", r#"{{} "\n"}"#),
    ] {
        let m = mm_parse::parse(&format!("s = {src}\n")).unwrap();
        let StmtKind::Assign(_, v) = &m.body[0].kind else {
            panic!()
        };
        let got = match &v.kind {
            ExprKind::Const(Const::Str(s)) => format!("{s:?}"),
            ExprKind::Const(Const::Bytes(b)) => format!("{b:?}"),
            ExprKind::FString(parts) => show(parts),
            other => panic!("{other:?}"),
        };
        assert_eq!(got, want, "{src:?}");
    }
}

/// Nesting just inside (and far past) the pre-scan's limits.
#[test]
fn deep_nesting_never_overflows() {
    let mut fstr = String::from("1");
    for _ in 0..99 {
        fstr = format!("f'{{{fstr}}}'");
    }
    let mut indent = String::new();
    for i in 0..99 {
        indent.push_str(&" ".repeat(i));
        indent.push_str("if x:\n");
    }
    indent.push_str(&" ".repeat(99));
    indent.push_str("pass\n");
    let cases = [
        format!("x = {}1\n", "lambda: ".repeat(995)),
        format!("x = {}1{}\n", "(".repeat(199), ")".repeat(199)),
        format!("x = {}1{}\n", "[".repeat(199), "]".repeat(199)),
        format!("x = {}1{}\n", "f(".repeat(199), ")".repeat(199)),
        format!("x = {}1{}\n", "{1: ".repeat(199), "}".repeat(199)),
        format!("x = {}1{}\n", "(lambda: ".repeat(199), ")".repeat(199)),
        format!("x = {}1{}\n", "(x for x in ".repeat(199), ")".repeat(199)),
        format!("x = {}1{}\n", "a[1:".repeat(199), "]".repeat(199)),
        format!(
            "def f():\n    return {}x{}\n",
            "(yield ".repeat(199),
            ")".repeat(199)
        ),
        format!("x = {}1\n", "-".repeat(990)),
        format!("x = {}1\n", "not ".repeat(990)),
        format!("x = {}c\n", "a if b else ".repeat(490)),
        format!("x = 2{}\n", "**2".repeat(495)),
        format!("x = 1{}\n", "+1".repeat(990)),
        format!("x = a{}\n", ".b".repeat(990)),
        format!(
            "x = {}1{}\n",
            "[-(not a if b else lambda: ".repeat(66),
            ")]".repeat(66)
        ),
        format!("x = {fstr}\n"),
        indent,
        format!("x = {}1{}\n", "(".repeat(10_000), ")".repeat(10_000)),
        format!("x = {}1\n", "lambda: ".repeat(20_000)),
    ];
    for src in cases {
        let head: String = src.chars().take(40).collect();
        let r = parse_on_small_stack(src);
        assert!(
            matches!(
                r,
                Ok(()) | Err("RecursionError" | "SyntaxError" | "IndentationError")
            ),
            "{head}: {r:?}"
        );
    }
}

#[test]
fn brackets_the_pre_scan_misreads_are_still_limited() {
    // The pre-scan read each of these differently from the tokenizer (a
    // comment ended by a lone `\r`, `\'` in a raw string, `\{` in an
    // f-string), so it let the brackets through, and the parser recursed
    // once per bracket: 7 MiB of stack for 3,000 in a release build (the
    // `compile` fuzz target). The tokenizer now stops at 200, as CPython's.
    let deep = "(".repeat(10_000);
    for src in [
        format!("x = 1 #\rx = {deep}\n"),
        format!("x = r'\\'' + {deep}\n"),
        format!("x = f'\\{{{deep}\n"),
    ] {
        let head: String = src.chars().take(16).collect();
        assert_eq!(parse_on_small_stack(src), Err("SyntaxError"), "{head:?}");
    }
}

/// Parses on a thread with room for the deepest nesting the pre-scan lets
/// through (debug builds use several times the release stack per level).
fn parse_on_small_stack(src: String) -> Result<(), &'static str> {
    let stack = if cfg!(debug_assertions) {
        6 << 20
    } else {
        1 << 20
    };
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || mm_parse::parse(&src).map(|_| ()).map_err(|e| e.kind))
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn deeply_nested_expressions_raise_recursion_error() {
    for src in [
        format!("x = {}1\n", "-".repeat(300)),
        format!("x = {}1\n", "not ".repeat(300)),
        format!("x = {}a\n", "lambda: ".repeat(300)),
        format!("x = {}1{}\n", "[".repeat(199), "]".repeat(199)),
    ] {
        let head: String = src.chars().take(20).collect();
        assert_eq!(parse_on_small_stack(src), Err("RecursionError"), "{head}");
    }
    // 199 parentheses add no AST depth.
    let parens = format!("x = {}1{}\n", "(".repeat(199), ")".repeat(199));
    assert_eq!(parse_on_small_stack(parens), Ok(()));
}

#[test]
fn unsupported_constructs_are_reported() {
    for (src, msg) in [
        (
            "x = '\\N{BULLET}'\n",
            "micro-monty does not support named Unicode escapes (\\N{...})",
        ),
        (
            "x = f'\\N{BULLET} {y}'\n",
            "micro-monty does not support named Unicode escapes (\\N{...})",
        ),
        ("x = t'{y}'\n", "micro-monty does not support t-strings"),
        // A `str` is UTF-8: no lone surrogates, as CPython keeps them.
        (
            "x = 'ab\\ud800'\n",
            "micro-monty does not support surrogate escapes (\\ud800 at position 2): its strings are UTF-8",
        ),
        (
            "x = f'{y}\\U0000DFFF'\n",
            "micro-monty does not support surrogate escapes (\\U0000DFFF at position 0): its strings are UTF-8",
        ),
        (
            "type X = int\n",
            "micro-monty does not support type aliases",
        ),
        (
            "from __future__ import braces\n",
            "future feature braces is not defined",
        ),
    ] {
        let e = mm_parse::parse(src).unwrap_err();
        assert_eq!((e.kind, e.msg.as_str()), ("SyntaxError", msg), "{src}");
    }
    // A bytes literal keeps `\N` as written.
    let m = mm_parse::parse("x = b'\\N{X}'\n").unwrap();
    let StmtKind::Assign(_, v) = &m.body[0].kind else {
        panic!()
    };
    assert_eq!(v.kind, ExprKind::Const(Const::Bytes(b"\\N{X}".to_vec())));
}

#[test]
fn compatibility_characters_in_names_are_normalized() {
    let m = mm_parse::parse("\u{FB01}x = \u{FF41}\n").unwrap();
    let StmtKind::Assign(t, v) = &m.body[0].kind else {
        panic!()
    };
    assert_eq!(t[0].kind, ExprKind::Name("fix".into()));
    assert_eq!(v.kind, ExprKind::Name("a".into()));
}

#[test]
fn names_needing_other_normalization_are_rejected() {
    // Combining marks of Indic scripts are fine as they are.
    assert!(mm_parse::parse("\u{928}\u{92e}\u{938}\u{94d}\u{924}\u{947} = 1\n").is_ok());
    for (src, c) in [
        ("\u{FF7F} = 1\n", "('\u{FF7F}', U+FF7F)"),
        ("cafe\u{301} = 1\n", "('\u{301}', U+0301)"),
        ("x = \u{01CB}\n", "('\u{01CB}', U+01CB)"),
    ] {
        let e = mm_parse::parse(src).unwrap_err();
        assert_eq!(
            e.msg,
            format!("micro-monty does not support identifiers that need NFKC normalization {c}")
        );
    }
}
