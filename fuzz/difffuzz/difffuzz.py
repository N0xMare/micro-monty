"""Differential fuzzer: micro-monty (`mm run`) against CPython 3.14.

A grammar-based generator writes deterministic subset programs (ints and
bigints, floats, strs, lists, dicts, tuples, sets; comprehensions,
functions, closures, classes with dunders, generators, try/except, f-strings,
slicing, string methods, sorting with keys, match, bounded async-for/async-with
and explicitly closed async generators), one per seed. Each
top-level block runs inside `try`/`except Exception` and prints the
exception's type and message, so one program checks many outcomes. Both
engines run every program; stdout, the last stderr line (an uncaught
exception) and the exit status must agree. A divergence is minimized
(blocks, then lines) and saved as `divergences/seed-<N>.py` with both
outputs in its header (non-printable characters escaped, so the file stays
one valid source).

Usage (normally through `cargo xtask difffuzz`):
    python3.14 fuzz/difffuzz/difffuzz.py --mm target/release/mm \\
        [--seeds N] [--start S] [--jobs J] [--timeout T] [--out DIR] [--minutes M]
    python3.14 fuzz/difffuzz/difffuzz.py --show SEED    # print one program

Programs avoid what is allowed to differ: set iteration order (sets print
through `sorted`), `hash`/`id` values, object addresses (every class has
`__repr__`) and `is` on ints and strings.

Libm results may differ by 1 ulp. Generated `math.exp/log/log2/log10/sin/
atan` calls and float `**` (also `int ** -k`) go through `_T`, the identity.
A program whose outputs differ is counted `ulp`, not divergent, when both
engines agree after rerunning with `_T` snapping each such result to a
4-ulp grid (at offsets 0 and 2, so two values 1 ulp apart meet under one)
and the unsnapped values they recorded are pairwise equal or 1 ulp apart.
Exact operations (arithmetic, `sqrt`, `fabs`, int `**`, `round`, parsing
and formatting) are never wrapped, so they must still agree exactly.

Sorting is a merge sort in mm and timsort in CPython (docs/user-guide.md
"Limitations"), so a list that cannot be sorted may fail on another pair.
A program is counted `sort`, not divergent, when its outputs agree once
each `not supported between instances of 'A' and 'B'` names its two types
in one order, and also agree when rerun with every sort replaced by one
Python insertion sort (`sort_order_only`).

With `MM_STEP_MS=N` in the environment, `mm run` also checks the step
bound (docs/spec.md §5): a start or resume more than N ms past a 1 ms
deadline exits with status 3 and `mm: a native step ran ...` on stderr,
which counts as a divergence. Leave room for the load of `--jobs`, such as
`MM_STEP_MS=250`.

`mm run` has a 256 MiB heap (docs/spec.md §11). A program is counted
`memory` when mm's first differing line names `MemoryError` and CPython,
rerun alone, peaked above 128 MiB (`heap_limit`).
"""

import argparse
import ast
import collections
import math
import multiprocessing
import os
import random
import re
import subprocess
import sys
import tempfile
import time

INTS = [0, 1, 2, 3, 5, 7, 10, 42, 100, 255, -1, -3, -17, 1000]
# Parenthesized: `{a} ** ({b} % 7)` must not read `2 ** 64 ** k` as
# `2 ** (64 ** k)` (heap-limit MemoryErrors in mm, CPython timeouts).
BIGS = ['(2 ** 64)', '(-(2 ** 70))', '(10 ** 30 + 7)', '(3 ** 50)', '((1 << 100) - 1)', '(-(10 ** 25))']
FLOATS = ['0.1', '1.5', '-2.25', '3.0', '1e10', '1e-05', '0.0', '-0.0', '2.5', '1e300', '7.125']
STRS = ["''", "'a'", "'hello'", "'Hello World'", "'a,b,,c'", "'  pad  '", "'\\u00df'",
        "'caf\\u00e9'", "'123'", "'x\\ny'", "'AbC'", "'mississippi'", "'-42'", "'3.5'",
        "'tab\\there'", "'\\U0001f600 ok'"]
INT_BINOPS = ['+', '-', '*', '//', '%', '&', '|', '^']
CMP = ['<', '<=', '==', '!=', '>', '>=']
STR_TRANSFORMS = ['upper', 'lower', 'strip', 'title', 'capitalize', 'swapcase', 'lstrip', 'rstrip', 'casefold']
STR_PREDICATES = ['isdigit', 'isalpha', 'isspace', 'isupper', 'islower', 'isalnum', 'isidentifier',
                  'istitle', 'isnumeric', 'isdecimal']
INT_FMT = ['', '>8', '<6', '^9', '08', '+', ',', '_', 'x', '#x', 'o', 'b', '#b', 'X', '08b', 'e',
           '.2f', '10.3e', '%', 'n', '+08,', 'c']
FLOAT_FMT = ['', '.2f', '.0f', '>10.3f', 'e', '.3e', 'g', '.4g', '%', '.1%', '+.2f', ',.2f',
             '08.2f', 'G', 'r', '.10g']
STR_FMT = ['', '>8', '<8', '^8', '*^9', '.2', '10.3', '!r', '!r:>12', '!s', '!a']

# Results of libm functions and float `**` (correctly rounded in neither
# CPython nor mm, so a platform's libm may be 1 ulp from mm) pass through
# `_T`, the identity in a normal run. On a divergence, `within_ulp` reruns
# both engines with `_T` snapping each such result to a grid of 4 ulps, at
# two offsets so that values 1 ulp apart meet under one of them, and
# recording the unsnapped values. Exact operations are never wrapped.
T_IDENTITY = '_T = lambda v: v'
T_SNAP = """_R = []
def _T(v):
    _R.append(v)
    if type(v) is not float or v != v or v in (math.inf, -math.inf) or v == 0:
        return v
    m, e = math.frexp(v)
    q = round((int(m * 2 ** 53) - OFF) / 4) * 4 + OFF
    return math.ldexp(q, e - 53)"""
T_REPORT = 'print("_R", [x.hex() if type(x) is float else repr(x) for x in _R])'


class Gen:
    """Writes one program from a seed."""

    def __init__(self, seed):
        self.r = random.Random(seed)
        self.vars = {}  # name -> type
        self.funcs = []  # (name, arg types, result type)
        self.classes = []  # names of classes with the standard dunders
        self.n = 0
        self.depth = 0

    # ---------------------------------------------------------------- utils

    def pick(self, seq):
        return self.r.choice(seq)

    def chance(self, p):
        return self.r.random() < p

    def fresh(self, prefix='v'):
        self.n += 1
        return f'{prefix}{self.n}'

    def var(self, ty):
        names = [n for n, t in self.vars.items() if t == ty]
        return self.pick(names) if names else None

    # ---------------------------------------------------------- expressions

    def expr(self, ty, d=0):
        if d > 3:
            return self.leaf(ty)
        return getattr(self, 'e_' + ty)(d + 1)

    def leaf(self, ty):
        v = self.var(ty)
        if v is not None and self.chance(0.6):
            return v
        return {
            'int': lambda: str(self.pick(INTS)),
            'float': lambda: self.pick(FLOATS),
            'str': lambda: self.pick(STRS),
            'bool': lambda: self.pick(['True', 'False']),
            'list': lambda: '[' + ', '.join(str(self.pick(INTS)) for _ in range(self.r.randint(0, 5))) + ']',
            'slist': lambda: '[' + ', '.join(self.pick(STRS) for _ in range(self.r.randint(0, 4))) + ']',
            'dict': lambda: '{' + ', '.join(f"'k{i}': {self.pick(INTS)}" for i in range(self.r.randint(0, 4))) + '}',
            'tuple': lambda: '(' + ''.join(f'{self.pick(INTS)}, ' for _ in range(self.r.randint(0, 4))) + ')',
            'set': lambda: '{' + ', '.join(str(self.pick(INTS)) for _ in range(self.r.randint(1, 5))) + '}',
        }[ty]()

    def e_int(self, d):
        k = self.r.randint(0, 22)
        a = lambda: self.expr('int', d)
        if k == 0:
            return self.pick(BIGS)
        if k == 1:
            return f'({a()} {self.pick(INT_BINOPS)} {a()})'
        if k == 2:
            return f'({a()} ** ({a()} % 7))'
        if k == 3:
            return f'({a()} << ({a()} % 70))'
        if k == 4:
            return f'({a()} >> ({a()} % 9))'
        if k == 5:
            return f'{self.pick(["-", "~", "+"])}{a()}'
        if k == 6:
            return f'len({self.expr(self.pick(["str", "list", "dict", "tuple", "set", "slist"]), d)})'
        if k == 7:
            return f'int({self.expr("str", d)})'
        if k == 8:
            return f'int({self.expr("float", d)})'
        if k == 9:
            return f'round({self.expr("float", d)})'
        if k == 10:
            return f'sum({self.expr("list", d)})'
        if k == 11:
            return f'{self.pick(["max", "min"])}({self.expr("list", d)})'
        if k == 12:
            return f'{self.expr("list", d)}[{a()}]'
        if k == 13:
            return f"{self.expr('dict', d)}[{self.pick(['k0', 'k1', 'k2', 'zz'])!r}]"
        if k == 14:
            return f'{self.expr("str", d)}.{self.pick(["find", "count", "rfind", "index"])}({self.expr("str", d)})'
        if k == 15:
            return f'({a()} if {self.expr("bool", d)} else {a()})'
        if k == 16:
            return f'divmod({a()}, {a()})[{self.r.randint(0, 1)}]'
        if k == 17:
            return f'pow({a()}, {self.r.randint(0, 40)}, {self.pick(["7", "1000", "2 ** 61 - 1", "-5"])})'
        if k == 18:
            return f'abs({a()}).bit_length()'
        if k == 19:
            return f'int({self.expr("str", d)}, {self.pick(["0", "2", "16", "36", "10"])})'
        if k == 20 and self.funcs:
            f = [x for x in self.funcs if x[2] == 'int']
            if f:
                name, args, _ = self.pick(f)
                return f'{name}({", ".join(self.expr(t, d) for t in args)})'
        if k == 21:
            return f'math.{self.pick(["floor", "ceil", "trunc", "isqrt", "factorial", "gcd"])}' + (
                f'({self.expr("float", d)})' if self.chance(0.4) else f'(abs({a()}) % 30)')
        return self.leaf('int')

    def e_float(self, d):
        k = self.r.randint(0, 10)
        a = lambda: self.expr('float', d)
        if k == 0:
            return f'({self.expr("int", d)} / {self.expr("int", d)})'
        if k == 1:
            return f'({a()} {self.pick(["+", "-", "*", "/", "//", "%"])} {a()})'
        if k == 2:
            return f'float({self.expr(self.pick(["int", "str"]), d)})'
        if k == 3:
            return f'round({a()}, {self.r.randint(-2, 4)})'
        if k == 4:
            fn = self.pick(["sqrt", "exp", "log", "sin", "atan", "log2", "fabs", "log10"])
            call = f'math.{fn}({a()})'
            # sqrt and fabs are exact; the others go through libm.
            return call if fn in ('sqrt', 'fabs') else f'_T({call})'
        if k == 5:
            return f'_T({a()} ** {self.pick(["2", "0.5", "-1", "3"])})'
        if k == 6:
            return f'{self.pick(["-", "+"])}{a()}'
        if k == 7:
            return f'abs({a()})'
        if k == 8:
            return f'math.{self.pick(["pi", "e", "inf", "tau"])}'
        return self.leaf('float')

    def e_str(self, d):
        k = self.r.randint(0, 20)
        s = lambda: self.expr('str', d)
        if k == 0:
            return f'({s()} + {s()})'
        if k == 1:
            return f'({s()} * ({self.expr("int", d)} % 4))'
        if k == 2:
            return f'{s()}[{self.slice(d)}]'
        if k == 3:
            return f'{s()}.{self.pick(STR_TRANSFORMS)}()'
        if k == 4:
            return f'{s()}.replace({s()}, {s()})'
        if k == 5:
            return f'{s()}.{self.pick(["center", "ljust", "rjust"])}({self.r.randint(0, 12)}, {self.pick(["chr(42)", "chr(45)"])})'
        if k == 6:
            return f'{s()}.zfill({self.r.randint(0, 9)})'
        if k == 7:
            return f'{s()}.join({self.expr("slist", d)})'
        if k == 8:
            return f'str({self.expr(self.pick(["int", "float", "list", "tuple", "bool", "dict"]), d)})'
        if k == 9:
            return f'repr({self.expr(self.pick(["str", "float", "slist", "tuple", "dict"]), d)})'
        if k == 10:
            return self.fstring(d)
        if k == 11:
            return f"'{self.pick(['%d', '%5.2f', '%s|%r', '%x', '%-6s|', '%05d', '%e', '%%%d'])}' % {self.pct_args(d)}"
        if k == 12:
            return f'chr({self.r.randint(32, 0x2fff)})'
        if k == 13:
            return f'{s()}[{self.expr("int", d)}]'
        if k == 14:
            return f"{s()}.split({self.pick(['', 'None', repr(','), repr(' '), repr('s')])})[{self.r.randint(-2, 2)}]"
        if k == 15:
            return f"{s()}.{self.pick(['partition', 'rpartition'])}({self.pick([repr(','), repr('s'), repr(' ')])})[{self.r.randint(0, 2)}]"
        if k == 16:
            return f"{s()}.{self.pick(['removeprefix', 'removesuffix', 'strip', 'lstrip'])}({s()})"
        if k == 17:
            return f"'{{}}-{{!r}}-{{:>4}}'.format({self.expr('int', d)}, {s()}, {self.expr('int', d)})"
        if k == 18:
            return f"{s()}.{self.pick(['expandtabs', 'title', 'swapcase'])}()"
        if k == 19 and self.funcs:
            f = [x for x in self.funcs if x[2] == 'str']
            if f:
                name, args, _ = self.pick(f)
                return f'{name}({", ".join(self.expr(t, d) for t in args)})'
        return self.leaf('str')

    def pct_args(self, d):
        return '(' + ', '.join(self.expr(self.pick(['int', 'float', 'str']), d) for _ in range(self.r.randint(1, 2))) + ',)'

    def fstring(self, d):
        parts = []
        for _ in range(self.r.randint(1, 3)):
            ty = self.pick(['int', 'float', 'str', 'list'])
            e = self.expr(ty, d + 1).replace("'", '"') if ty != 'str' else self.var('str') or "'q'"
            if "'" in e or '\\' in e:
                e = self.var(ty) or {'int': '7', 'float': '2.5', 'str': 'None', 'list': '[1]'}[ty]
            spec = {'int': INT_FMT, 'float': FLOAT_FMT, 'str': STR_FMT, 'list': ['', '!r']}[ty]
            sp = self.pick(spec)
            if sp.startswith('!'):
                parts.append('{' + e + sp + '}')
            elif sp:
                parts.append('{' + e + ':' + sp + '}')
            elif self.chance(0.2):
                parts.append('{' + e + '=}')
            else:
                parts.append('{' + e + '}')
            parts.append(self.pick(['', ' ', '|', ' and ']))
        return "f'" + ''.join(parts) + "'"

    def slice(self, d):
        r = lambda: self.pick(['', str(self.r.randint(-6, 8))])
        if self.chance(0.4):
            return f'{r()}:{r()}:{self.pick(["1", "2", "-1", "-2", "3"])}'
        return f'{r()}:{r()}'

    def e_bool(self, d):
        k = self.r.randint(0, 11)
        if k == 0:
            ty = self.pick(['int', 'float', 'str', 'tuple', 'list'])
            if ty == 'float':
                return f'({self.expr("float", d)} {self.pick(CMP)} {self.expr("int", d)})'
            return f'({self.expr(ty, d)} {self.pick(CMP)} {self.expr(ty, d)})'
        if k == 1:
            return f'({self.expr("int", d)} in {self.expr(self.pick(["list", "tuple", "set"]), d)})'
        if k == 2:
            return f'({self.expr("str", d)} in {self.expr(self.pick(["str", "slist"]), d)})'
        if k == 3:
            return f'{self.expr("str", d)}.{self.pick(STR_PREDICATES)}()'
        if k == 4:
            return f'{self.pick(["all", "any"])}({self.expr("list", d)})'
        if k == 5:
            return f'(not {self.expr("bool", d)})'
        if k == 6:
            return f'({self.expr("bool", d)} {self.pick(["and", "or"])} {self.expr("bool", d)})'
        if k == 7:
            return f'isinstance({self.expr(self.pick(["int", "str", "list"]), d)}, {self.pick(["int", "str", "list", "(int, str)", "bool", "object"])})'
        if k == 8:
            return f'{self.expr("str", d)}.{self.pick(["startswith", "endswith"])}({self.expr("str", d)})'
        if k == 9:
            return f'({self.expr("int", d)} < {self.expr("int", d)} <= {self.expr("int", d)})'
        if k == 10:
            return f'bool({self.expr(self.pick(["int", "str", "list", "dict", "float"]), d)})'
        return self.leaf('bool')

    def e_list(self, d):
        k = self.r.randint(0, 16)
        a = lambda: self.expr('list', d)
        if k == 0:
            return '[' + ', '.join(self.expr('int', d) for _ in range(self.r.randint(0, 4))) + ']'
        if k == 1:
            x = self.fresh('i')
            return f'[{self.sub(self.expr("int", d), x)} for {x} in {self.iterable(d)}' + (f' if {x} % {self.r.randint(2, 4)}' if self.chance(0.5) else '') + ']'
        if k == 2:
            return f'sorted({a()}' + self.pick(['', ', reverse=True', ', key=abs', ', key=lambda q: (q % 3, -q)', ', key=str']) + ')'
        if k == 3:
            return f'list(range({self.r.randint(-3, 5)}, {self.r.randint(-5, 12)}, {self.pick(["1", "2", "-1", "3", "-2"])}))'
        if k == 4:
            return f'({a()} + {a()})'
        if k == 5:
            return f'({a()} * {self.r.randint(0, 3)})'
        if k == 6:
            return f'{a()}[{self.slice(d)}]'
        if k == 7:
            return f'[ord(c) for c in {self.expr("str", d)}]'
        if k == 8:
            return f'list(map(lambda q: q * {self.r.randint(-3, 3)} + 1, {a()}))'
        if k == 9:
            return f'list(filter(lambda q: q > {self.r.randint(-2, 5)}, {a()}))'
        if k == 10:
            return f'[a * b for a, b in zip({a()}, {a()})]'
        if k == 11:
            return f'[i + v for i, v in enumerate({a()}, {self.r.randint(0, 2)})]'
        if k == 12:
            return f'list({self.expr("dict", d)}.values())'
        if k == 13:
            return f'sorted({self.expr("set", d)})'
        if k == 14:
            return f'list(reversed({a()}))'
        if k == 15:
            return f'[len(w) for w in {self.expr("slist", d)}]'
        return self.leaf('list')

    def e_slist(self, d):
        k = self.r.randint(0, 6)
        if k == 0:
            return f'{self.expr("str", d)}.split({self.pick(["", repr(","), repr(" ")])})'
        if k == 1:
            return f'sorted({self.expr("slist", d)}, key={self.pick(["len", "str.lower", "lambda w: w[::-1]", "lambda w: (-len(w), w)"])})'
        if k == 2:
            return f'list({self.expr("str", d)})'
        if k == 3:
            return f'list({self.expr("dict", d)})'
        if k == 4:
            return f'[str(q) for q in {self.expr("list", d)}]'
        if k == 5:
            return f'{self.expr("str", d)}.splitlines()'
        return self.leaf('slist')

    def e_dict(self, d):
        k = self.r.randint(0, 7)
        if k == 0:
            return '{' + ', '.join(f'{self.expr("str", d)}: {self.expr("int", d)}' for _ in range(self.r.randint(0, 3))) + '}'
        if k == 1:
            x = self.fresh('k')
            return f'{{{x}: len({x}) for {x} in {self.expr("slist", d)}}}'
        if k == 2:
            return f'dict(zip({self.expr("slist", d)}, {self.expr("list", d)}))'
        if k == 3:
            return f'({self.expr("dict", d)} | {self.expr("dict", d)})'
        if k == 4:
            return f'{{str(q): q for q in {self.expr("list", d)}}}'
        if k == 5:
            return f'dict(sorted({self.expr("dict", d)}.items(), key=lambda kv: kv[1]))'
        return self.leaf('dict')

    def e_tuple(self, d):
        k = self.r.randint(0, 6)
        if k == 0:
            return '(' + ''.join(f'{self.expr(self.pick(["int", "str"]), d)}, ' for _ in range(self.r.randint(0, 3))) + ')'
        if k == 1:
            return f'tuple({self.expr("list", d)})'
        if k == 2:
            return f'divmod({self.expr("int", d)}, {self.expr("int", d)})'
        if k == 3:
            return f'({self.expr("tuple", d)} + {self.expr("tuple", d)})'
        if k == 4:
            return f'{self.expr("tuple", d)}[{self.slice(d)}]'
        if k == 5:
            return f'tuple(sorted({self.expr("dict", d)}.items()))'
        return self.leaf('tuple')

    def e_set(self, d):
        k = self.r.randint(0, 6)
        if k == 0:
            return f'set({self.expr("list", d)})'
        if k == 1:
            return f'({self.expr("set", d)} {self.pick(["|", "&", "-", "^"])} {self.expr("set", d)})'
        if k == 2:
            x = self.fresh('s')
            return f'{{{x} % {self.r.randint(2, 7)} for {x} in {self.expr("list", d)}}}'
        if k == 3:
            return f'frozenset({self.expr("list", d)}) | {{{self.r.randint(0, 9)}}}'
        return self.leaf('set')

    def iterable(self, d):
        return self.pick([f'range({self.r.randint(0, 8)})', self.expr('list', d), f'range({self.r.randint(-3, 3)}, {self.r.randint(4, 9)})'])

    @staticmethod
    def sub(e, x):
        return f'({e} + {x})'

    def shown(self, ty, e):
        """Text that prints `e` deterministically."""
        if ty == 'set':
            return f'sorted({e})'
        return e

    # ----------------------------------------------------------- statements

    def block(self):
        k = self.r.randint(0, 20)
        return getattr(self, 'b_' + ['assign', 'print', 'if', 'for', 'while', 'func', 'closure',
                                     'gen', 'class', 'try', 'match', 'mutate', 'unpack', 'with',
                                     'print', 'assign', 'sorting', 'recursion', 'comprehension',
                                     'exception_class', 'async_cleanup'][k])()

    def b_assign(self):
        ty = self.pick(['int', 'float', 'str', 'list', 'dict', 'tuple', 'set', 'bool', 'slist'])
        name = self.var(ty) if self.chance(0.3) else None
        name = name or self.fresh()
        e = self.expr(ty)
        self.vars[name] = ty
        if self.chance(0.2) and ty in ('int', 'str', 'list'):
            op = {'int': self.pick(['+=', '-=', '*=', '//=', '%=', '**=', '|=', '<<=']), 'str': '+=', 'list': '+='}[ty]
            rhs = self.expr(ty) if op not in ('**=', '<<=') else str(self.r.randint(0, 9))
            return [f'{name} = {e}', f'{name} {op} {rhs}', f'print({name!r}, {self.shown(ty, name)})']
        return [f'{name} = {e}', f'print({name!r}, {self.shown(ty, name)})']

    def b_print(self):
        ty = self.pick(['int', 'float', 'str', 'list', 'dict', 'tuple', 'set', 'bool', 'slist'])
        args = [self.shown(ty, self.expr(ty))]
        if self.chance(0.3):
            args.append(f"sep={self.pick(STRS)}")
        if self.chance(0.2):
            args.append(f"end={self.pick(STRS)}")
        return [f'print({", ".join(args)})']

    def body(self, n=None, indent='    '):
        out = []
        for _ in range(n or self.r.randint(1, 3)):
            self.depth += 1
            stmts = self.pick([self.b_print, self.b_assign, self.b_mutate])() if self.depth > 2 else self.block()
            self.depth -= 1
            out.extend(stmts)
        return [indent + line for line in out]

    def b_if(self):
        out = [f'if {self.expr("bool")}:'] + self.body()
        if self.chance(0.5):
            out += [f'elif {self.expr("bool")}:'] + self.body()
        if self.chance(0.6):
            out += ['else:'] + self.body()
        return out

    def b_for(self):
        x = self.fresh('i')
        self.vars[x] = 'int'
        out = [f'for {x} in {self.iterable(0)}:'] + self.body()
        if self.chance(0.3):
            out.insert(1, f'    if {x} == {self.r.randint(0, 5)}:')
            out.insert(2, f'        {self.pick(["break", "continue"])}')
        if self.chance(0.3):
            out += ['else:', f'    print("loop done", {x!r})']
        return out

    def b_while(self):
        c = self.fresh('n')
        self.vars[c] = 'int'
        return [f'{c} = {self.r.randint(0, 6)}', f'while {c} > 0:', f'    {c} -= 1'] + self.body() + [f'print({c!r}, {c})']

    def b_func(self):
        name = self.fresh('f')
        nargs = self.r.randint(0, 3)
        args = [self.pick(['int', 'str', 'list']) for _ in range(nargs)]
        params = [f'p{i}' for i in range(nargs)]
        ret = self.pick(['int', 'str'])
        saved = dict(self.vars)
        for p, t in zip(params, args):
            self.vars[p] = t
        defaults = ''
        sig = ', '.join(params)
        if nargs and self.chance(0.3):
            sig += f', *rest, kw={self.r.randint(0, 5)}'
        body = self.body(self.r.randint(0, 2))
        r = self.expr(ret)
        self.vars = saved
        out = [f'def {name}({sig}):'] + body + [f'    return {r}']
        self.funcs.append((name, args, ret))
        call = f'{name}({", ".join(self.expr(t) for t in args)})'
        out.append(f'print({call!r}, {call})')
        return out

    def b_closure(self):
        mk = self.fresh('make')
        return [
            f'def {mk}(start):',
            '    total = start',
            '    def step(by=1):',
            '        nonlocal total',
            f'        total = total {self.pick(["+", "*", "-"])} by',
            '        return total',
            '    return step',
            f's = {mk}({self.expr("int")})',
            f'print(s(), s({self.r.randint(-3, 5)}), s(by={self.r.randint(0, 4)}))',
            f'fs = [lambda q, i=i: q * i for i in range({self.r.randint(1, 4)})]',
            f'print([f({self.r.randint(1, 3)}) for f in fs])',
        ]

    def b_gen(self):
        g = self.fresh('gen')
        out = [f'def {g}(n):', '    for k in range(n):']
        out.append(f'        if k % {self.r.randint(2, 4)} == 0:')
        out.append(f'            yield k * {self.r.randint(-3, 3)}')
        if self.chance(0.4):
            out.append(f'        yield from {self.pick(["(k, k)", "[]", "range(k % 3)"])}')
        if self.chance(0.3):
            out.append('    return "done"')
        n = self.r.randint(0, 9)
        out.append(f'print(list({g}({n})), sum({g}({n})), max({g}({n}), default=None))')
        out.append(f'it = {g}({n})')
        out.append('print(next(it, "empty"), next(it, "empty"), list(it))')
        out.append(f'print(sum(x * x for x in range({self.r.randint(0, 9)}) if x % 2))')
        return out

    def b_class(self):
        c = self.fresh('C')
        base = self.pick(self.classes) if self.classes and self.chance(0.3) else None
        out = [f'class {c}({base}):' if base else f'class {c}:']
        out += [
            f'    tag = {self.pick(STRS)}',
            '    def __init__(self, v, w=0):',
            ('        super().__init__(v, w)' if base else '        self.v = v'),
            '        self.w = w',
            '    def __repr__(self):',
            f"        return f'{c}({{self.v!r}}, {{self.w}})'",
            '    def __eq__(self, other):',
            f'        return isinstance(other, {c}) and self.v == other.v',
            '    def __hash__(self):',
            '        return hash(self.v)',
            '    def __lt__(self, other):',
            '        return (self.v, self.w) < (other.v, other.w)',
            '    def __add__(self, other):',
            f'        return {c}(self.v + (other.v if isinstance(other, {c}) else other), self.w)',
            '    def __len__(self):',
            '        return abs(self.w) % 5',
            '    def __iter__(self):',
            '        return iter(range(len(self)))',
            '    def __getitem__(self, i):',
            '        return [self.v, self.w][i]',
            '    def __contains__(self, x):',
            '        return x == self.v',
            '    @property',
            '    def double(self):',
            '        return self.v * 2',
            '    @staticmethod',
            '    def make(n):',
            f'        return {c}(n, -n)',
            '    @classmethod',
            '    def named(cls):',
            '        return cls.__name__',
        ]
        if self.chance(0.3):
            out += ['    def __bool__(self):', '        return self.w > 0']
        self.classes.append(c)
        objs = [f'{c}({self.expr("int")}, {self.r.randint(-4, 4)})' for _ in range(3)]
        out += [
            f'objs = [{", ".join(objs)}, {c}.make({self.r.randint(0, 5)})]',
            'print(objs, sorted(objs), max(objs), min(objs, key=lambda o: o.w))',
            'a, b = objs[0], objs[1]',
            'print(a == b, a != b, a < b, a + b, a + 5, len(a), list(a), a[1], 3 in a, bool(a))',
            f'print(a.double, {c}.named(), a.tag, isinstance(a, {c}), sorted(set(objs)))',
            f'print({{o: o.w for o in objs}}[objs[0]], getattr(a, "missing", "dflt"))',
        ]
        if self.chance(0.4):
            out.append('print(a.nope)')
        return out

    def b_try(self):
        risky = self.pick([
            f'{self.expr("int")} // {self.expr("int")}',
            f'{self.expr("list")}[{self.expr("int")}]',
            f"{self.expr('dict')}[{self.expr('str')}]",
            f'int({self.expr("str")})',
            f'{self.expr("str")}.index({self.expr("str")})',
            f'raise ValueError({self.expr("str")})',
            f'raise KeyError({self.expr("int")})',
            f'{self.expr("tuple")}[{self.expr("int")}]',
            f'float({self.expr("str")})',
            f'[1, 2].remove({self.expr("int")})',
            f'{self.expr("str")} + {self.expr("int")}',
            f'{self.expr("list")} < {self.expr("str")}',
            'undefined_name',
            f'{self.expr("int")}.nope',
            f'math.sqrt({self.expr("float")})',
            f'chr({self.expr("int")})',
            f'_T({self.expr("int")} ** -{self.r.randint(1, 3)})',
        ])
        stmt = risky if risky.startswith('raise') else f'print({risky})'
        exc = self.pick(['Exception', 'ValueError', 'ArithmeticError', 'LookupError', '(TypeError, ValueError)', 'KeyError', 'IndexError', 'ZeroDivisionError'])
        out = ['try:', f'    {stmt}', f'except {exc} as e:', '    print("caught", type(e).__name__, e, e.args)']
        if self.chance(0.4):
            out += ['else:', '    print("no error")']
        if self.chance(0.5):
            out += ['finally:', f'    print("finally", {self.expr("int")})']
        if self.chance(0.2):
            out = ['def risky():'] + ['    ' + line for line in out] + ['    return "after"', 'print(risky())']
        return out

    def b_exception_class(self):
        e = self.fresh('Err')
        return [
            f'class {e}({self.pick(["Exception", "ValueError", "KeyError", "LookupError"])}):',
            '    def __init__(self, msg, code=0):',
            '        super().__init__(msg)',
            '        self.code = code',
            'try:',
            '    try:',
            f'        raise {e}({self.expr("str")}, {self.r.randint(0, 9)})',
            f'    except {e} as inner:',
            '        raise RuntimeError("wrapped") from inner',
            'except RuntimeError as outer:',
            '    print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)',
            f'print(issubclass({e}, Exception), {e}.__mro__[1].__name__)',
        ]

    def b_match(self):
        subj = self.pick([self.expr('int'), self.expr('list'), self.expr('str'), self.expr('dict'), self.expr('tuple'), 'None', '(1, "x")'])
        cases = self.r.sample([
            ['case 0 | 1 | 2:', '    print("small")'],
            ['case int(n) if n > 10:', '    print("big", n)'],
            ['case int() | float():', '    print("number")'],
            ['case [] | ():', '    print("empty")'],
            ['case [x]:', '    print("one", x)'],
            ['case [x, y, *rest]:', '    print("many", x, y, rest)'],
            ['case (a, "x"):', '    print("pair", a)'],
            ["case {'k0': v, **others}:", '    print("map", v, others)'],
            ['case {}:', '    print("any mapping")'],
            ['case str(s) if len(s) > 3:', '    print("long", s[:3])'],
            ["case 'a' | 'hello':", '    print("word")'],
            ['case None:', '    print("none")'],
            ['case list(l):', '    print("list", len(l))'],
        ], self.r.randint(2, 6))
        out = [f'match {subj}:']
        for c in cases:
            out += ['    ' + line for line in c]
        out += ['    case _:', '        print("other")']
        return out

    def b_mutate(self):
        lst = self.var('list') or 'lst0'
        if lst == 'lst0':
            self.vars['lst0'] = 'list'
            pre = ['lst0 = [3, 1, 2]']
        else:
            pre = []
        op = self.pick([
            f'{lst}.append({self.expr("int")})',
            f'{lst}.extend({self.expr("list")})',
            f'{lst}.insert({self.r.randint(-3, 5)}, {self.expr("int")})',
            f'print({lst}.pop({self.pick(["", "0", "-1", "5"])}))',
            f'{lst}.remove({self.expr("int")})',
            f'{lst}.sort({self.pick(["", "reverse=True", "key=lambda q: -q", "key=str"])})',
            f'{lst}.reverse()',
            f'del {lst}[{self.slice(0)}]',
            f'{lst}[{self.slice(0)}] = {self.expr("list")}',
            f'{lst}[{self.r.randint(-3, 3)}] = {self.expr("int")}',
            f'print({lst}.index({self.expr("int")}), {lst}.count({self.expr("int")}))',
            f'{lst} *= {self.r.randint(0, 2)}',
        ])
        d = self.var('dict')
        if d and self.chance(0.4):
            op = self.pick([
                f"{d}[{self.expr('str')}] = {self.expr('int')}",
                f"print({d}.pop({self.expr('str')}, 'nope'))",
                f"print({d}.get({self.expr('str')}), {d}.setdefault('k9', 9))",
                f"{d}.update({self.expr('dict')})",
                f"del {d}[{self.expr('str')}]",
                f"print(list({d}.items())[:3], {d}.popitem() if {d} else None)",
            ])
            return pre + [op, f'print({d})']
        return pre + [op, f'print({lst})']

    def b_unpack(self):
        return self.pick([
            [f'a, *b = {self.expr("list")}', 'print(a, b)'],
            [f'*a, b = {self.expr("str")}', 'print(a, b)'],
            [f'(a, b), c = {self.expr("tuple")}, {self.expr("int")}', 'print(a, b, c)'],
            [f'print(*{self.expr("list")}, *{self.expr("str")}, sep=",")'],
            [f'print({{**{self.expr("dict")}, "z": 1}}, [*{self.expr("tuple")}, *{self.expr("list")}])'],
            [f'if (w := {self.expr("int")}) > {self.r.randint(-5, 50)}:', '    print("walrus", w)'],
            [f'x = y = {self.expr("list")}', 'x.append(0)', 'print(x is y, y)'],
        ])

    def b_with(self):
        c = self.fresh('Ctx')
        return [
            f'class {c}:',
            '    def __init__(self, name):',
            '        self.name = name',
            '    def __enter__(self):',
            '        print("enter", self.name)',
            '        return self',
            '    def __exit__(self, kind, value, tb):',
            '        print("exit", self.name, kind.__name__ if kind else None, value)',
            f'        return {self.pick(["True", "False", "None"])}',
            f'with {c}("a") as x, {c}("b"):',
            f'    print("inside", x.name)',
            f'    {self.pick(["pass", "raise ValueError(7)", "print(1 / 0)", "print(x.name * 2)"])}',
            'print("after with")',
        ]

    def b_sorting(self):
        data = self.pick([self.expr('slist'), self.expr('list'), '[(1, "b"), (0, "c"), (1, "a"), (-2, "z")]'])
        key = self.pick(['None', 'len', 'str', 'lambda q: str(q)[::-1]', 'lambda q: (str(q), q)'])
        return [f'data = {data}', f'print(sorted(data, key={key}, reverse={self.pick(["True", "False"])}))',
                f'print(min(data, key={key}, default="-"), max(data, key={key}, default="-"))']

    def b_async_cleanup(self):
        """No implicit finalization: aclosing makes early-exit cleanup portable.

        Each loop has at most four yields; sleeps only yield to the scheduler,
        so event order is deterministic and no clock/host service is needed.
        """
        f = self.fresh('async_cleanup')
        n = self.r.randint(0, 4)
        stop = self.r.randint(0, 4)
        action = self.pick(['break', 'continue', 'return log', 'raise ValueError("stop")'])
        suppress = self.pick(['True', 'False'])
        return [
            'import asyncio',
            'from contextlib import aclosing',
            f'async def {f}():',
            '    log = []',
            '    class CM:',
            '        async def __aenter__(self):',
            '            await asyncio.sleep(0)',
            '            log.append("enter")',
            '            return 3',
            '        async def __aexit__(self, kind, value, tb):',
            '            await asyncio.sleep(0)',
            '            log.append(("exit", kind.__name__ if kind else None))',
            f'            return {suppress}',
            '    async def values():',
            '        try:',
            f'            for i in range({n}):',
            '                await asyncio.sleep(0)',
            '                yield i',
            '        finally:',
            '            await asyncio.sleep(0)',
            '            log.append("closed")',
            '    try:',
            '        async with CM() as offset:',
            '            async with aclosing(values()) as stream:',
            '                async for item in stream:',
            '                    log.append(item + offset)',
            f'                    if item == {stop}:',
            f'                        {action}',
            '                else:',
            '                    log.append("exhausted")',
            '    except ValueError:',
            '        log.append("caught")',
            '    return log',
            f'print(asyncio.run({f}()))',
        ]

    def b_recursion(self):
        f = self.fresh('rec')
        n = self.r.randint(0, 25)
        return [f'def {f}(n):', '    if n <= 1:', '        return n', f'    return {f}(n - 1) + {f}(n - 2)' if n < 20 else f'    return n * {f}(n - 1)',
                f'print({f}({n}))']

    def b_comprehension(self):
        return self.pick([
            [f'print({{k: v for k, v in zip({self.expr("slist")}, {self.expr("list")}) if v}})'],
            [f'print([[i * j for j in range({self.r.randint(0, 4)})] for i in range({self.r.randint(0, 4)})])'],
            [f'print(sorted({{c for c in {self.expr("str")} if c.isalpha()}}))'],
            [f'print([x for row in ({self.expr("list")}, {self.expr("list")}) for x in row if x % 2 == 0])'],
            [f'g = (q * 2 for q in {self.expr("list")})', 'print(next(g, None), list(g))'],
            [f'print({{q % 3: q for q in {self.expr("list")}}})'],
        ])

    def program(self):
        lines = ['import math', T_IDENTITY]
        for _ in range(self.r.randint(8, 22)):
            block = self.block()
            lines += ['try:'] + ['    ' + line for line in block] + [
                'except Exception as exc:', '    print("EXC", type(exc).__name__, exc)']
        return '\n'.join(lines) + '\n'


# --------------------------------------------------------------------- run

def run(cmd, path, timeout):
    env = dict(os.environ, PYTHONHASHSEED='0')
    env.pop('PYTHONSTARTUP', None)
    try:
        p = subprocess.run(cmd + [path], capture_output=True, timeout=timeout, env=env, stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return ('timeout', '', '')
    err = p.stderr.decode('utf-8', 'replace').replace(path, '<file>').strip().splitlines()
    return (p.returncode, p.stdout.decode('utf-8', 'replace'), err[-1] if err else '')


def outcome(args, source):
    with tempfile.NamedTemporaryFile('w', suffix='.py', delete=False, dir=args.tmp) as f:
        f.write(source)
        path = f.name
    try:
        py = run([args.python, '-X', 'utf8'], path, args.timeout)
        if py[0] == 'timeout':
            return None, None
        mm = run([args.mm, 'run'], path, args.timeout * 4)
        return py, mm
    finally:
        os.unlink(path)


def one_ulp(a, b):
    """Recorded `_T` values: equal, or floats at most 1 ulp apart."""
    if a == b:
        return True
    try:
        x, y = float.fromhex(a), float.fromhex(b)
    except ValueError:
        return False
    return y in (math.nextafter(x, math.inf), math.nextafter(x, -math.inf))


def within_ulp(args, source):
    """Do the engines agree once each libm result is allowed 1 ulp?"""
    for off in (0, 2):
        snapped = source.replace(T_IDENTITY, T_SNAP.replace('OFF', str(off)), 1) + T_REPORT + '\n'
        py, mm = outcome(args, snapped)
        if py is None or py[0] != mm[0] or py[2] != mm[2]:
            continue
        pyl, mml = py[1].splitlines(), mm[1].splitlines()
        if not pyl or not mml or pyl[:-1] != mml[:-1]:
            continue
        rp, rm = ast.literal_eval(pyl[-1][3:]), ast.literal_eval(mml[-1][3:])
        if len(rp) == len(rm) and all(map(one_ulp, rp, rm)):
            return True
    return False


MIXED_PAIR = re.compile(r"not supported between instances of ('[^']*') and ('[^']*')")
PY_SORT = """def _sorted(it, key=None, reverse=False):
    if isinstance(it, (set, frozenset)):
        it = sorted(it, key=lambda x: (type(x).__name__, repr(x)))
    xs = list(it)
    if reverse:
        xs.reverse()
    ks = xs if key is None else [key(x) for x in xs]
    order = []
    for i in range(len(xs)):
        j = len(order)
        while j and ks[i] < ks[order[j - 1]]:
            j -= 1
        order.insert(j, i)
    out = [xs[i] for i in order]
    if reverse:
        out.reverse()
    return out
def _sort(lst, key=None, reverse=False):
    lst[:] = _sorted(lst, key=key, reverse=reverse)
"""


def pairs_unordered(text):
    """`text` with the two type names of each `not supported between
    instances of 'A' and 'B'` message in a fixed order."""
    return MIXED_PAIR.sub(lambda m: 'not supported between instances of %s and %s' % tuple(sorted(m.groups())), text)


def sort_order_only(args, source, py, mm):
    """Is the difference only which pair an unsortable list fails on?

    mm's sort is a stable merge sort, CPython's is timsort (docs/user-guide.md
    "Limitations"), so sorting a mixed-type list can report `'int' and
    'str'` where CPython reports `'str' and 'int'`. Both outputs must agree
    once each message's type pair is put in one order, and also when the
    program reruns with every `sorted(...)`/`x.sort(...)` replaced by one
    Python insertion sort (a set is first put in a portable order: its
    iteration order is allowed to differ), so a swapped pair from a plain
    `a < b` still counts as divergent."""
    if py[0] != mm[0] or pairs_unordered(py[1]) != pairs_unordered(mm[1]) or \
            pairs_unordered(py[2]) != pairs_unordered(mm[2]):
        return False
    rewritten = re.sub(r'(?<![\w.])(\w+)\.sort\(', r'_sort(\1, ', re.sub(r'(?<![\w.])sorted\(', '_sorted(', source))
    if rewritten == source:
        return False
    py2, mm2 = outcome(args, rewritten.replace(T_IDENTITY, T_IDENTITY + '\n' + PY_SORT, 1))
    return py2 is not None and py2 == mm2


# Peak RSS in bytes. Linux: VmHWM, since ru_maxrss also counts the parent
# at fork (a worker holding a big output). macOS: ru_maxrss, in bytes.
RSS_REPORT = """import os as _o, resource as _r
if _o.path.exists('/proc/self/status'):
    print('_RSS', [int(l.split()[1]) * 1024 for l in open('/proc/self/status') if l.startswith('VmHWM:')][0])
else:
    print('_RSS', _r.getrusage(_r.RUSAGE_SELF).ru_maxrss)
"""


def heap_limit(args, source, py, mm):
    """Did mm stop at its heap limit on data CPython needed that much for?

    `mm run` has the default 256 MiB heap (docs/spec.md §11) and raises
    `MemoryError` past it; CPython has no such limit. The first differing
    mm line must name `MemoryError`, and CPython, rerun alone, must peak
    above 128 MiB, so a `MemoryError` on small data stays divergent."""
    if 'MemoryError' not in first_difference(py, mm)[2]:
        return False
    with tempfile.NamedTemporaryFile('w', suffix='.py', delete=False, dir=args.tmp) as f:
        f.write(source + RSS_REPORT)
        path = f.name
    try:
        out = run([args.python, '-X', 'utf8'], path, args.timeout)[1].splitlines()
    finally:
        os.unlink(path)
    return bool(out) and out[-1].startswith('_RSS ') and int(out[-1][5:]) > 128 << 20


def known_difference(args, source, py, mm):
    """Which allowed difference, if any, explains the outputs."""
    if within_ulp(args, source):
        return 'ulp'
    if sort_order_only(args, source, py, mm):
        return 'sort'
    if heap_limit(args, source, py, mm):
        return 'memory'
    return None


def differs(args, source, sig):
    py, mm = outcome(args, source)
    return (py is not None and py != mm and signature(py, mm) == sig
            and not known_difference(args, source, py, mm))


def minimize(args, source, sig):
    """Drops top-level blocks, then single lines, while the engines still
    differ the same way."""
    lines = source.splitlines()

    def blocks(ls):
        out, cur = [], []
        for line in ls:
            if line and not line[0].isspace() and not line.startswith(('except', 'else', 'elif', 'finally')) and cur:
                out.append(cur)
                cur = []
            cur.append(line)
        if cur:
            out.append(cur)
        return out

    bs = blocks(lines)
    i = 2  # keep `import math` and `_T`
    while i < len(bs):
        trial = bs[:i] + bs[i + 1:]
        if differs(args, '\n'.join(sum(trial, [])) + '\n', sig):
            bs = trial
        else:
            i += 1
    lines = sum(bs, [])
    i = 2
    while i < len(lines):
        trial = lines[:i] + lines[i + 1:]
        try:
            compile('\n'.join(trial), 'm', 'exec')
        except SyntaxError:
            i += 1
            continue
        if differs(args, '\n'.join(trial) + '\n', sig):
            lines = trial
        else:
            i += 1
    return '\n'.join(lines) + '\n'


def first_difference(py, mm):
    """The first differing stdout lines (or the exits and stderr)."""
    pyl, mml = py[1].splitlines(), mm[1].splitlines()
    for k in range(max(len(pyl), len(mml))):
        a = pyl[k] if k < len(pyl) else '<none>'
        b = mml[k] if k < len(mml) else '<none>'
        if a != b:
            return k + 1, a, b
    return 0, f'exit {py[0]}: {py[2]}', f'exit {mm[0]}: {mm[2]}'


def signature(py, mm):
    """Groups divergences: the differing lines with numbers blanked."""
    _, a, b = first_difference(py, mm)
    norm = lambda t: re.sub(r'\d+', '#', t)[:100]
    if 'has no attribute' in b or 'is not defined' in b or 'not supported' in b:
        return 'mm: ' + norm(b)
    return 'CPython: ' + norm(a) + ' | mm: ' + norm(b)


def check_seed(job):
    args, seed = job
    source = Gen(seed).program()
    py, mm = outcome(args, source)
    if py is None:
        return seed, 'skip', None
    if py == mm:
        return seed, 'same', None
    known = known_difference(args, source, py, mm)
    if known:
        return seed, known, None
    sig = signature(py, mm)
    with args.lock:
        seen = args.seen.get(sig, 0)
        args.seen[sig] = seen + 1
    if seen >= args.per_signature:
        return seed, 'diverge', (sig, None)
    small = minimize(args, source, sig)
    py, mm = outcome(args, small)
    if py is None or py == mm:
        small, (py, mm) = source, outcome(args, source)
    head = ['# difffuzz divergence, seed %d' % seed,
            '# CPython: exit %s, last stderr line: %s' % (py[0], commented(py[2])),
            '# mm:      exit %s, last stderr line: %s' % (mm[0], commented(mm[2]))]
    k, a, b = first_difference(py, mm)
    head += ['# first difference (stdout line %d, 0 = exit):' % k, '#   CPython: ' + commented(a[:300]),
             '#   mm:      ' + commented(b[:300]), '# signature: ' + commented(sig)]
    path = os.path.join(args.out, 'seed-%d.py' % seed)
    with open(path, 'w') as f:
        f.write('\n'.join(head) + '\n' + small)
    return seed, 'diverge', (sig, path)


def commented(text):
    """`text` for a `#` comment line: characters that are not printable
    (a null byte, a line break) escaped as in a str literal, so the saved
    seed stays one valid Python source."""
    return ''.join(c if c.isprintable() else repr(c)[1:-1] for c in str(text))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--mm')
    ap.add_argument('--python', default=os.environ.get('PYTHON') or 'python3.14')
    ap.add_argument('--seeds', type=int, default=100)
    ap.add_argument('--start', type=int, default=0)
    ap.add_argument('--jobs', type=int, default=max(1, (os.cpu_count() or 2) - 1))
    ap.add_argument('--timeout', type=float, default=10)
    ap.add_argument('--minutes', type=float, default=0, help='stop starting seeds after this long')
    ap.add_argument('--out', default=os.path.join(os.path.dirname(os.path.abspath(__file__)), 'divergences'))
    ap.add_argument('--show', type=int)
    ap.add_argument('--per-signature', type=int, default=2, help='divergences minimized and saved per signature')
    args = ap.parse_args()
    if args.show is not None:
        sys.stdout.write(Gen(args.show).program())
        return 0
    if not args.mm:
        ap.error('--mm is required')
    os.makedirs(args.out, exist_ok=True)
    args.tmp = tempfile.mkdtemp(prefix='difffuzz-')
    started = time.time()
    counts = {'same': 0, 'ulp': 0, 'sort': 0, 'memory': 0, 'diverge': 0, 'skip': 0}
    sigs = collections.Counter()
    manager = multiprocessing.Manager()
    args.seen, args.lock = manager.dict(), manager.Lock()
    seeds = range(args.start, args.start + args.seeds)
    with multiprocessing.Pool(args.jobs) as pool:
        for seed, status, found in pool.imap_unordered(check_seed, ((args, s) for s in seeds)):
            counts[status] += 1
            if found:
                sig, path = found
                sigs[sig] += 1
                if path:
                    print('diverge', seed, path, flush=True)
            if args.minutes and time.time() - started > args.minutes * 60:
                pool.terminate()
                break
    elapsed = time.time() - started
    done = sum(counts.values())
    print(f'difffuzz: {done} programs in {elapsed:.0f} s: {counts["same"]} same, '
          f'{counts["ulp"]} same within 1 ulp of libm results, '
          f'{counts["sort"]} same but for the pair a mixed-type sort fails on, '
          f'{counts["memory"]} at mm\'s heap limit, '
          f'{counts["diverge"]} divergent ({len(sigs)} signatures), {counts["skip"]} skipped '
          '(CPython timeout)')
    for sig, n in sigs.most_common():
        print(f'{n:6}  {sig}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
