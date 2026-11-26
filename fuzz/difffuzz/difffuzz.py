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
        [--seeds N] [--start S] [--jobs J] [--timeout T] [--out DIR] [--minutes M] [--park]
    python3.14 fuzz/difffuzz/difffuzz.py --show SEED    # print one program

Half the blocks are `EXTRA_BLOCKS`: builtins called with values of every
type, format specs, bigints, str and bytes methods, struct, collections,
dict views, sets, builtin subclasses, operator dispatch, class protocols,
generator send/throw, argument binding, exceptions, itertools, lru_cache,
dataclasses, json, re and asyncio. Most print one probe a line through
`_p`, which prints an exception's type and message, or the type's name
for a result whose repr shows an address.

Programs avoid what is allowed to differ: set iteration order (sets print
through `sorted`), `hash`/`id` values, object addresses (every class has
`__repr__`; a signature writes a `0x…` address as `#`) and `is` on ints
and strings.

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

With `--park`, a program both engines agree on is run three more ways on
mm through `mm exec --json --main-module --digest`: unparked
(`--report-fuel`), parked every N fuel units in one process with a snapshot
at every stop (`--park-every N --snapshot-continue`), and as a chain of
snapshots, each restored by `mm resume` in a new process
(`--snapshot-every N --snapshot-dir`). N is 1, 7, 61 or 1009 by seed, or
more for a long program (at most about 200 and 25 stops); each parked run
must give the unparked run's output, result, exception, exit, fuel and
final-state digest (docs/spec.md §13), else it is a `park` divergence,
saved unminimized.

`mm run` has a 256 MiB heap (docs/spec.md §11). A program is counted
`memory` when mm's first differing line names `MemoryError` and CPython,
rerun alone, peaked above 128 MiB (`heap_limit`).
"""

import argparse
import ast
import collections
import json
import math
import multiprocessing
import os
import random
import re
import shutil
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
# Blocks for stdlib modules and object protocols, as likely as the
# statement blocks above together.
EXTRA_BLOCKS = ['builtin_errors', 'format_spec', 'floats', 'bigint', 'strings', 'bytes', 'struct',
                'collections', 'views', 'sets', 'subclass_builtin', 'dispatch', 'class_protocols',
                'generator_protocol', 'args', 'exceptions', 'iterstd', 'lru', 'records', 'json', 'regex',
                'asyncio', 'metaclass', 'dates_typing_text']
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
            if e.startswith('{'):
                e = ' ' + e  # `{{` would be an escaped brace, not a dict display
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
        return getattr(self, 'b_' + self.pick(['assign', 'print', 'if', 'for', 'while', 'func', 'closure',
                                                'gen', 'class', 'try', 'match', 'mutate', 'unpack', 'with',
                                                'print', 'assign', 'sorting', 'recursion', 'comprehension',
                                                'exception_class', 'async_cleanup'] + EXTRA_BLOCKS))()

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
            f'({self.expr("int")}).nope',
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

    # ------------------------------------------ stdlib and protocol blocks

    def probes(self, exprs, params=''):
        """Each expression in its own `try`, printing its value (a type name
        where the repr holds an address) or its exception."""
        # One call per line, so the minimizer drops probes one at a time.
        return ['def _p(f):', '    try:', '        v = f()', '        r = repr(v)', '    except Exception as e:',
                '        print(type(e).__name__, e)', '    else:', "        print(type(v).__name__ if ' at 0x' in r else r)"] + [
            f'_p(lambda {params}: {e})' for e in exprs]

    def b_builtin_errors(self):
        vals = ['None', '0', '-3', '2.5', '-0.0', '1e300', "'ab'", "''", "'3'", '[1, 2]', '[]', '(1,)',
                '{1: 2}', '{1, 2}', "b'xy'", 'True', '1j', '(2 ** 70)', 'range(3)', 'len', 'int', "'\\u00e9'",
                "{'a': [1]}", 'frozenset()', 'slice(1, 2)', '...', 'NotImplemented', "bytearray(b'q')"]
        forms = ['len({x})', 'type(hash({x})).__name__', '{x}[0]', "{x}['k']", '{x}[-1:]', '{x}()', '{x} + {y}', '{x} * {y}',
                 '{x} < {y}', '{x} == {y}', '-{x}', '(0 if {x} is True else ~{x})', 'abs({x})', 'int({x})', 'float({x})', 'str({x})',
                 'bool({x})', 'list({x})', 'tuple({x})', 'sorted({x})', 'dict({x})', 'set({x})', 'sum({x})',
                 'max({x})', 'min({x}, default=None)', 'list(reversed({x}))', 'list(zip({x}, {y}, strict=True))',
                 'round({x})', 'round({x}, {y})', 'divmod({x}, {y})', 'pow({x}, {y})', '{x} % {y}', '{x} // {y}',
                 '{x} / {y}', '{x} ** {y}', '{x} in {y}', '{x}.count({y})', '{x}.index({y})',
                 "format({x}, {spec!r})", 'chr({x})', 'ord({x})', 'hex({x})', 'bin({x})', 'oct({x})',
                 "''.join({x})", '{x}.split({y})', 'range({x})', 'range({x}, {y})', 'isinstance({x}, {y})',
                 'getattr({x}, {y})', 'getattr({x}, "real", 7)', 'bytes({x})', 'repr({x})', 'ascii({x})',
                 'any({x})', 'callable({x})', 'type({x})({y})', '{x} & {y}', '{x} << {y}', '{x} >> {y}',
                 'int({x}, {y})', 'str({x}, "ascii")', 'complex({x})', '{x}.real', 'iter({x}, {y})',
                 '[*{x}]', '{{**{x}}}', 'enumerate({x}, {y})', 'list(map({x}, {y}))', 'issubclass({x}, {y})',
                 'max({x}, {y})', 'sorted([{x}, {y}])', 'bytearray({x})', '{x}.__class__.__name__',
                 'f"{{{x}!r:>{{5}}}}"', 'print({x}, sep={y})', '{x}.bit_length()', 'list({x})[{y}]']
        specs = ['', 'd', 's', '>4', 'x', '.1f', ',', '=+5', 'c', 'z.1f', '#o', '_b', '^']
        exprs = []
        for _ in range(self.r.randint(4, 10)):
            exprs.append(self.pick(forms).format(x='x', y='y', spec=self.pick(specs)))
        # Bound as defaults, so no literal meets CPython's compile-time SyntaxWarnings.
        return self.probes(exprs, f'x=({self.pick(vals)}), y=({self.pick(vals)})')

    def fmt_spec(self):
        p = ''
        if self.chance(0.3):
            p += self.pick(['', '*', '0', '#', ' ', 'x']) + self.pick(['<', '>', '^', '='])
        elif self.chance(0.3):
            p += self.pick(['<', '>', '^', '='])
        if self.chance(0.3):
            p += self.pick(['+', '-', ' '])
        if self.chance(0.1):
            p += 'z'
        if self.chance(0.2):
            p += '#'
        if self.chance(0.2):
            p += '0'
        if self.chance(0.4):
            p += str(self.r.randint(0, 14))
        if self.chance(0.25):
            p += self.pick([',', '_'])
        if self.chance(0.4):
            p += '.' + str(self.r.randint(0, 12))
            if self.chance(0.1):
                p += self.pick([',', '_'])
        if self.chance(0.7):
            p += self.pick(list('bcdeEfFgGnosxX%') + [''])
        return p

    def b_format_spec(self):
        vals = ['0', '7', '-42', '255', '(10 ** 20 + 3)', '(-(2 ** 65))', 'True', '0.0', '-0.0', '1.5', '2.675',
                '1e16', '-1234.5678', '1e-07', '123456789.0', '0.1', 'float("inf")', '-float("inf")', "'abc'",
                "''", "'\\u00e9t\\u00e9'", '2.5', '0.5', '1e22', '9.999999', '5e-324', '(1 << 53) + 1']
        exprs = []
        for _ in range(self.r.randint(4, 10)):
            v, s = self.pick(vals), self.fmt_spec()
            exprs.append(self.pick([f'format({v}, {s!r})', f"'{{:{s}}}'.format({v})", f"f'{{{v}:{s}}}'"]))
        pct = ['%d', '%5.2f', '%-8s|', '%+05d', '%x', '%#o', '%e', '%.3g', '%10.4e', '%c', '%r', '%a', '%%',
               '%i', '%u', '%X', '%#x', '% d', '%.0f', '%*d', '%(k)s', '%-+7.2f', '%G', '%.12g', '%5s']
        for _ in range(self.r.randint(0, 4)):
            parts = [self.pick(pct) for _ in range(self.r.randint(1, 2))]
            spec = ''.join(parts)
            want = sum(0 if p == '%%' else 2 if '*' in p else 1 for p in parts) + self.pick([0, 0, 0, 1, -1])
            args = ', '.join(self.pick(vals) for _ in range(max(want, 1)))
            exprs.append(f"{spec!r} % ({args},)" if '(k)' not in spec else f"{spec!r} % {{'k': {self.pick(vals)}}}")
        return self.probes(exprs)

    def b_floats(self):
        """Exact float operations on varied bit patterns: repr, rounding,
        hex, ratios, conversions (no libm)."""
        xs = []
        for _ in range(self.r.randint(2, 5)):
            k = self.r.randint(0, 4)
            if k == 0:
                xs.append(f'float.fromhex({hex(self.r.getrandbits(52) | (1 << 52))[2:]!r}) * 2.0 ** {self.r.randint(-1080, 1020)}')
            elif k == 1:
                xs.append(repr(round(self.r.uniform(-1000, 1000), self.r.randint(0, 6))))
            elif k == 2:
                xs.append(f'{self.r.randint(-10 ** 6, 10 ** 6)} / {self.r.randint(1, 1000)}')
            elif k == 3:
                xs.append(f'{self.r.randint(1, 9)}e{self.r.randint(-330, 310)}')
            else:
                xs.append(self.pick(FLOATS))
        exprs = []
        for x in xs:
            for _ in range(self.r.randint(2, 4)):
                exprs.append(self.pick([
                    f'repr({x})', f'str({x})', f'({x}).hex()', f'({x}).as_integer_ratio()', f'({x}).is_integer()',
                    f'int({x})', f'round({x}, {self.r.randint(-5, 20)})', f'round({x})', f'math.floor({x})',
                    f'divmod({x}, {self.pick(xs)})', f'({x}) % {self.pick(xs)}', f'({x}) // {self.pick(xs)}',
                    f'math.fmod({x}, {self.pick(xs)})', f'float(repr({x})) == {x}', f'math.ldexp({x}, {self.r.randint(-60, 60)})',
                    f'math.frexp({x})', f'math.modf({x})', f'math.copysign(1, {x})', f'({x}) * {self.pick(xs)}',
                    f'({x}) + {self.pick(xs)}', f'({x}) / {self.pick(xs)}', f'math.isclose({x}, {self.pick(xs)})',
                    f'math.nextafter({x}, 0)', f'math.ulp({x})', f'({x}) == {self.r.randint(-5, 5)}',
                    f'({x}) < {self.pick(BIGS)}', f'hash({x}) == hash(int({x})) if ({x}).is_integer() else None',
                    f'math.fsum([{x}, {self.pick(xs)}, -{x}])', f'math.trunc({x})', f'math.ceil({x})',
                    f"float(format({x}, '.17g')) == {x}", f'fractions.Fraction({x})', f'decimal.Decimal({x})'])
                )
        return ['import fractions, decimal'] + self.probes(exprs)

    def bigint(self):
        k = self.r.randint(0, 3)
        if k == 0:
            return str(self.r.choice([-1, 1]) * self.r.getrandbits(self.r.randint(1, 400)))
        if k == 1:
            return f'({self.pick(["-", ""])}(1 << {self.r.randint(0, 300)}) {self.pick(["+", "-"])} {self.r.randint(0, 3)})'
        if k == 2:
            return f'({self.pick(["-", ""])}10 ** {self.r.randint(0, 120)})'
        return str(self.r.randint(-300, 300))

    def b_bigint(self):
        exprs = []
        for _ in range(self.r.randint(3, 9)):
            a, b = self.bigint(), self.bigint()
            exprs.append(self.pick([
                f'{a} * {b}', f'{a} // {b}', f'{a} % {b}', f'divmod({a}, {b})', f'{a} & {b}', f'{a} | {b}',
                f'{a} ^ {b}', f'~{a}', f'{a} >> {self.r.randint(0, 200)}', f'{a} << {self.r.randint(0, 100)}',
                f'pow({a}, {self.r.randint(0, 300)}, {b})', f'pow({a}, -1, {b})', f'math.gcd({a}, {b})',
                f'math.lcm({a}, {b})', f'math.isqrt(abs({a}))', f'str({a})', f'hex({a})', f'oct({a})',
                f'bin({a})', f"format({a}, '{self.pick([',', '_', '_x', '#b', '>60', '.3e', 'n'])}')",
                f'({a}).bit_count()', f'({a}).bit_length()', f'{a} / {b}', f'float({a})', f'{a} == float({a})',
                f'{a} < float({b})', f'int(str({a}))', f'int(hex({a}), 16)', f'int(bin({a}), 0)',
                f"({a}).to_bytes(60, '{self.pick(['big', 'little'])}', signed=True)",
                f"int.from_bytes(({a}).to_bytes(60, 'big', signed=True), 'big', signed={self.pick(['True', 'False'])})",
                f'{a} ** {self.r.randint(0, 5)}', f'round({a}, {self.r.randint(-30, 2)})', f'math.comb(abs({a}) % 200, {self.r.randint(0, 50)})',
                f'math.factorial(abs({a}) % 120)', f"int('{'9' * self.r.choice([4299, 4300, 4301])}')", f'str(10 ** {self.r.choice([4299, 4300, 4301])})',
                f'({a}).as_integer_ratio()', f'{a} // {self.pick(["2.5", "-0.5", "1e10"])}',
                f'math.prod([{a}, {b}, {self.bigint()}])', f'sum([{a}, {b}], {self.bigint()})']))
        return self.probes(exprs)

    def b_strings(self):
        strs = STRS + ["'\\u0130stanbul'", "'stra\\u00dfe'", "'\\ufb01ne'", "'\\u01c5ungla'", "'\\u00bd\\u00b2\\u0663'",
                       "\"it's o'k\"", "'\\u03a3\\u03b1\\u03c3'", "'a\\r\\nb\\x0bc\\x1cd\\u2028e'", "'  \\t\\n'",
                       "'\\u1e9e'", "'\\u0149'", "'ABC def_1'", "('x' * 20)", "'a\\tb\\tc'", "'\\u00c5\\u212b'"]
        s = lambda: self.pick(strs)
        n = lambda: self.r.randint(-4, 12)
        exprs = []
        for _ in range(self.r.randint(4, 10)):
            exprs.append(self.pick([
                f'{s()}.split({s()}, {n()})', f'{s()}.rsplit({self.pick(["None", s()])}, {n()})',
                f'{s()}.splitlines({self.pick(["True", "False"])})', f'{s()}.find({s()}, {n()}, {n()})',
                f'{s()}.rfind({s()}, {n()})', f'{s()}.count({s()}, {n()}, {n()})', f'{s()}.replace({s()}, {s()}, {n()})',
                f'{s()}.expandtabs({n()})', f'{s()}.translate(str.maketrans({s()}, {s()}))',
                f"{s()}.translate({{97: 'AA', 98: None, 99: 120}})", f'{s()}.center({n()}, {s()})',
                f'{s()}.startswith(({s()}, {s()}), {n()})', f"{s()}.encode({self.pick(['\"utf-8\"', '\"ascii\"', '\"latin-1\"', '\"utf-16-le\"'])}, {self.pick(['\"strict\"', '\"replace\"', '\"ignore\"', '\"xmlcharrefreplace\"', '\"backslashreplace\"'])})",
                f'{s()}.casefold()', f'{s()}.upper()', f'{s()}.lower()', f'{s()}.title()', f'{s()}.capitalize()',
                f'{s()}.swapcase()', f'{s()}.isprintable()', f'{s()}.isidentifier()', f'{s()}.isnumeric()',
                f'{s()}.isdecimal()', f'{s()}.isdigit()', f'{s()}.istitle()', f'{s()}.isspace()',
                f'{s()}.partition({s()})', f'{s()}.rpartition({s()})', f'{s()} < {s()}', f'sorted({s()})',
                f'{s()}.ljust({n()}, {s()})', f'{s()}.zfill({n()})', f'{s()}[{n()}:{n()}:{self.pick(["1", "-1", "2", "0"])}]',
                f'{s()}.format_map({{"a": 1}})', f'{s()}.removeprefix({s()})', f'{s()}.strip({s()})',
                f'{s()}.index({s()}, {n()})', f'len({s()}.upper())', f'[ord(c) for c in {s()}.lower()]',
                f'{s()}.endswith({s()}, {n()}, {n()})', f'repr({s()})', f'ascii({s()})', f'{s()} * {n()}',
                f'{s()}.join([{s()}, {s()}])', f'str.maketrans({s()}, {s()}, {s()})', f'max({s()})',
                f"'{{0}}{{1!r:^{{2}}}}{{x}}'.format({s()}, {s()}, {n()}, x={s()})", f'{s()}.encode().decode("utf-8")',
                f'{s()}.rjust({n()})', f'{s()}.rstrip({s()})', f'{s()}.lstrip()']))
        return self.probes(exprs)

    def b_bytes(self):
        bs = ["b''", "b'abc'", "b'\\x00\\xff\\x7f'", "b'hello world'", "b'a,b,,c'", 'bytes(range(5))',
              "'caf\\u00e9'.encode()", "b'  Mixed Case\\t'", "b'\\x80abc'", "bytes(3)", "b'123'"]
        b = lambda: self.pick(bs)
        n = lambda: self.r.randint(-3, 9)
        exprs = []
        for _ in range(self.r.randint(4, 9)):
            exprs.append(self.pick([
                f'{b()}.hex()', f'{b()}.hex({self.pick(["\":\"", "\"-\", 2", "\" \", -1"])})', f"bytes.fromhex({self.pick(['\"0a ff\"', '\"zz\"', '\"abc\"', '\"\"', '\" 01 02 \"'])})",
                f'{b()}.split({self.pick(["", "b\",\"", "None, 1"])})', f'{b()}.upper()', f'{b()}[{n()}]', f'{b()}[{n()}:{n()}]',
                f'{b()} + {b()}', f'{b()} * {n()}', f'{b()}.find({b()})', f'{b()}.decode("latin-1")',
                f'{b()}.decode("utf-8", {self.pick(["\"replace\"", "\"strict\"", "\"ignore\"", "\"backslashreplace\""])})',
                f'list({b()})', f'{b()}.replace({b()}, {b()})', f'{b()}.strip()', f'{b()}.count({b()})',
                f'{b()}.center({n()}, b"*")', f'{b()}.isalpha()', f'{b()}.title()', f'{b()}.partition(b" ")',
                f'{b()} < {b()}', f'{b()} in {b()}', f'{n()} in {b()}', f"b'%d|%s|%r|%x' % ({n()}, {b()}, {b()}, {n()})",
                f"({self.bigint()}).to_bytes({n()}, '{self.pick(['big', 'little'])}', signed={self.pick(['True', 'False'])})",
                f"int.from_bytes({b()}, '{self.pick(['big', 'little'])}', signed={self.pick(['True', 'False'])})",
                f'bytes([{n()}, {self.pick(["255", "256", "-1", "65"])}])', f'{b()}.zfill({n()})', f'bytearray({b()}) + {b()}',
                f'{b()}.join([{b()}, {b()}])', f'{b()}.splitlines()', f'{b()}.expandtabs()', f'{b()}.rjust({n()})',
                f'{b()}.startswith({b()})', f'bytes({b()}.decode("latin-1"), "latin-1")', f'{b()}.swapcase()',
                f'{b()}.translate(bytes.maketrans(b"ab", b"xy"))', f'{b()}.isdigit()', f'{b()}.removeprefix({b()})']))
        out = self.probes(exprs)
        ops = self.r.sample([f'ba.extend({b()})', f'ba[{n()}] = {self.pick(["0", "65", "300"])}', f'del ba[{n()}:{n()}]',
                             f'ba.insert({n()}, 66)', 'print(ba.pop())', 'ba.reverse()', f'ba[1:3] = {b()}',
                             f'ba += {b()}', 'ba *= 2', f'print(ba.remove(97))', f'ba[::2] = {b()}'], self.r.randint(1, 4))
        out += [f'ba = bytearray({b()})']
        for op in ops:
            out += ['try:', f'    {op}', 'except Exception as e:', '    print(type(e).__name__, e)']
        return out + ['print(ba, len(ba), bytes(ba))']

    def b_struct(self):
        codes = {'b': (-130, 130), 'B': (-2, 258), 'h': (-33000, 33000), 'H': (-1, 70000), 'i': (-2 ** 31 - 1, 2 ** 31),
                 'I': (-1, 2 ** 32), 'q': (-2 ** 63 - 1, 2 ** 63), 'Q': (0, 2 ** 64), 'l': (-5, 5), 'L': (0, 9), 'n': (-9, 9)}
        order = self.pick(['<', '>', '!', '=', '@', ''])
        fmt, vals = order, []
        for _ in range(self.r.randint(1, 4)):
            c = self.pick(list(codes) + ['e', 'f', 'd', '?', 's', 'p', 'c', 'x'])
            rep = self.pick(['', '', '', '2', '3', '0'])
            if c == 'n' and order not in ('@', ''):
                c = 'q'
            fmt += rep + c
            count = int(rep or 1)
            if c in 'sp':
                vals.append(self.pick(["b'abc'", "b''", "b'xy'", "b'\\xff\\x00'"]))
            elif c == 'x':
                pass
            else:
                for _ in range(count):
                    if c in codes:
                        lo, hi = codes[c]
                        vals.append(str(self.pick([lo, hi, 0, 1, -1, self.r.randint(lo, hi)])) if self.chance(0.3)
                                    else str(self.r.randint(max(lo, 0) if c.isupper() else lo, hi)))
                    elif c in 'efd':
                        vals.append(self.pick(['1.5', '-0.0', '65504.0', '65520.0', '1e40', '3.4e38', '0.1', '1e-10',
                                               'float("inf")', '6e-08', '2.0 ** -24', '1', '123456789.123']))
                    elif c == '?':
                        vals.append(self.pick(['True', '0', '[]', '"x"']))
                    else:
                        vals.append(self.pick(["b'a'", "b'ab'", "'a'", '65']))
        pad = self.r.randint(0, 4)
        off = self.r.randint(0, pad)
        return [
            'import struct',
            f'fmt = {fmt!r}',
            'print(struct.calcsize(fmt))',
            f'data = struct.pack(fmt, {", ".join(vals)})',
            'print(data, len(data), struct.unpack(fmt, data))',
            f"print(struct.unpack_from(fmt, b'\\x01' * {off} + data + b'\\x00' * {pad - off}, {off}))",
            'print(list(struct.iter_unpack(fmt, data * 2)) if data else None)',
            'buf = bytearray(len(data) + 2)',
            f'struct.pack_into(fmt, buf, {self.r.randint(0, 2)}, {", ".join(vals)})',
            'print(buf, struct.Struct(fmt).size, struct.Struct(fmt).format)',
            f'print(struct.unpack(fmt, data[:{self.r.randint(-3, 3)}]))',
        ]

    def b_collections(self):
        L, S, SL = (lambda: self.expr('list')), (lambda: self.expr('str')), (lambda: self.expr('slist'))
        return self.pick([
            ['import collections', f'c = collections.Counter({S()})', f'd = collections.Counter({L()})',
             'print(c, d.most_common(2), c.total(), c["zz"])',
             f'print(d + d, d - collections.Counter({L()}), c | collections.Counter({S()}), c & collections.Counter({S()}))',
             f'c.subtract({S()})', 'print(c, +c, -c, sorted(d.elements()), c == collections.Counter(dict(c)))',
             f'c.update({SL()})', 'print(c.most_common(), c <= c + c, list(c)[:3])'],
            ['import collections', f'q = collections.deque({L()}, maxlen={self.pick(["None", "1", "3", "5"])})',
             f'q.rotate({self.r.randint(-4, 4)})', f'q.appendleft({self.expr("int")})', f'q.extend({L()})',
             f'q.extendleft({L()})', 'print(q, q.maxlen, len(q), list(reversed(q)), q[0] if q else None, q[-1] if q else None)',
             f'print(q.pop() if q else None, q.popleft() if q else None, q.count({self.expr("int")}), q)',
             f'q.insert({self.r.randint(-2, 3)}, 99) if q.maxlen is None else None', f'q.remove({self.expr("int")})', 'print(q, q * 2, q + q, 3 in q)'],
            ['import collections', 'dd = collections.defaultdict(list)', f'for w in {SL()}:', '    dd[len(w)].append(w)',
             'print(dd, dict(dd), dd[99], sorted(dd))', 'ic = collections.defaultdict(int)', f'for ch in {S()}:', '    ic[ch] += 1',
             'print(ic, ic.default_factory, dd.copy() == dd, len(ic))'],
            ['import collections', f'od = collections.OrderedDict.fromkeys({S()}, 0)', f'od.move_to_end({S()[:3] or "q"!r}[:1] or "q", last={self.pick(["True", "False"])})',
             'print(od, list(reversed(od)), od == dict(od), od.popitem(last=False) if od else None)',
             f'od.update({self.expr("dict")})', 'print(od, od.popitem() if od else None, list(od.items())[:2], od | {"z": 1})'],
            ['import collections', f'P = collections.namedtuple("P", {self.pick(["\"x y z\"", "[\"x\", \"y\", \"z\"]", "\"x, y, z\""])}, defaults=[{self.r.randint(0, 3)}])',
             f'p = P({self.expr("int")}, {self.expr("str")})', 'print(p, p._replace(x=5), p._asdict(), P._fields, P._field_defaults)',
             f'print(p + (1,), p.x, p[1:], P._make([1, 2, 3]), p == (p.x, p.y, p.z), P(*range(3)) < P(0, 1, 3), len(p))',
             'x, y, z = p', 'print(x, y, z, p.index(p.y), p.count(0), repr(P), P(z=1, y=2, x=3))',
             'print(P(1, 2, 3, 4))'],
            ['import collections', f'cm = collections.ChainMap({self.expr("dict")}, {self.expr("dict")})',
             'print(cm, len(cm), sorted(cm), cm.get("k0"), dict(cm))', 'cm["k1"] = 7', 'cm2 = cm.new_child({"k0": -1})',
             'print(cm, cm2["k0"], cm2.parents == cm, list(cm2.items()))'],
        ])

    def b_views(self):
        D = lambda: self.expr('dict')
        return [
            f'd = {D()}', f'e = {D()}', 'k, i, v = d.keys(), d.items(), e.values()',
            f'print(sorted(k & e.keys()), sorted(k | {self.expr("slist")}), sorted(k - e), sorted(k ^ e.keys()))',
            "print(k == e.keys(), k <= e.keys(), k < {'k0', 'k1', 'k2', 'k3'}, ('k0', d.get('k0')) in i, len(v), k.isdisjoint(e))",
            f'print(sorted(i & e.items()), sorted(i - {{("k0", {self.pick(INTS)})}}), sorted(i | {{1}}, key=str), i == e.items())',
            f"d[{self.expr('str')}] = {self.expr('int')}",
            'print(list(k), list(reversed(i)), list(reversed(v)), repr(k), repr(v), repr(i))',
            f'print(dict.fromkeys({self.expr("slist")}, {self.expr("int")}), sorted(d.keys() - e.keys() - {{"k1"}}), {{"k0"}} & k)',
            f'print(sorted(k - {self.expr("set")}), [*i], max(k, default="-"))',
            "print(sorted(i ^ e.items()), i <= i | e.items(), {('k0', 1): 2}.keys() & i)",
            "print(sorted(k) == sorted(set(d)), sorted(k & ['k0', 'zz']), (1, 2) in i)",
            'it = iter(k)', "d['added'] = 0", 'print(next(it))',
        ]

    def b_sets(self):
        S, L = (lambda: self.expr('set')), (lambda: self.expr('list'))
        ops = [f'a.add({self.expr("int")})', f'a.discard({self.expr("int")})', f'a.remove({self.expr("int")})',
               f'a.update({L()}, {S()})', f'a.symmetric_difference_update({L()})', f'a.intersection_update({L()}, {S()})',
               f'a.difference_update({L()})', f'a -= {S()}', f'a |= {S()}', f'a &= {S()}', f'a ^= {S()}', 'a.clear()']
        out = [f'a = {S()}', f'b = {S()}',
               f'print(a.issubset(b), a <= b, a < b, a >= b, a > b, a == b, a.isdisjoint({L()}), a.issuperset({L()}))',
               f'print(sorted(a.symmetric_difference({L()})), sorted(a.union({L()}, b)), sorted(a.intersection(b, {L()})), sorted(a.difference({L()}, b)))',
               f'print(frozenset(a) == a, sorted(frozenset(a) | b), type(frozenset(a) | b).__name__, type(b | frozenset(a)).__name__, {{frozenset(a): 1}}.get(frozenset(sorted(a))))',
               f'print(a | {L()})', f'print(a.union())', f'print(sorted(set().union(*[{L()}, {L()}])), a <= frozenset(a), {self.expr("int")} in a)']
        for op in self.r.sample(ops, self.r.randint(1, 4)):
            out += ['try:', f'    {op}', 'except Exception as e:', '    print(type(e).__name__, e)', 'print(sorted(a))']
        return out

    def b_subclass_builtin(self):
        """Subclasses of builtin containers and scalars that override some
        dunders, used where CPython reads them and where it does not."""
        kind = self.pick(['list', 'dict', 'str', 'int', 'tuple', 'float', 'set'])
        c = self.fresh('Sub')
        over = {
            'list': ['__iter__', '__len__', '__getitem__', '__contains__', '__eq__', '__add__', '__repr__', '__bool__', '__lt__'],
            'dict': ['__getitem__', '__iter__', '__len__', 'keys', '__contains__', '__missing__', 'get', 'items', '__repr__', '__eq__'],
            'str': ['__str__', '__repr__', '__len__', '__getitem__', '__eq__', '__hash__', '__add__', '__contains__', 'upper', '__iter__', '__format__'],
            'int': ['__index__', '__int__', '__add__', '__radd__', '__repr__', '__str__', '__eq__', '__hash__', '__bool__', '__lt__', '__float__', '__format__'],
            'tuple': ['__iter__', '__len__', '__getitem__', '__repr__', '__eq__', '__lt__', '__contains__'],
            'float': ['__repr__', '__str__', '__float__', '__int__', '__eq__', '__add__', '__round__', '__format__'],
            'set': ['__iter__', '__len__', '__contains__', '__repr__', '__eq__', '__or__', '__le__'],
        }[kind]
        bodies = {
            '__iter__': 'return iter([7, 8])', '__len__': 'return 1', '__getitem__': 'return "item"',
            '__contains__': 'return True', '__eq__': 'return True', '__add__': 'return "added"', '__radd__': 'return "radded"',
            '__repr__': f'return "{c}!"', '__str__': 'return "s!"', '__bool__': 'return False', '__lt__': 'return True',
            'keys': 'return ["kk"]', '__missing__': 'return "missing"', 'get': 'return "got"', 'items': 'return [("i", 1)]',
            '__hash__': 'return 1', 'upper': 'return "UP"', '__index__': 'return 3', '__int__': 'return 4',
            '__float__': 'return 2.5', '__round__': 'return "rounded"', '__or__': 'return "or"', '__le__': 'return "le"',
            '__format__': 'return "fmt:" + spec',
        }
        chosen = self.r.sample(over, self.r.randint(1, 3))
        out = [f'class {c}({kind}):']
        for m in chosen:
            args = '(self, spec)' if m == '__format__' else '(self, *a)'
            out += [f'    def {m}{args}:', f'        {bodies[m]}']
        init = {'list': '[1, 2, 3]', 'dict': "{'a': 1, 'b': 2}", 'str': "'abc'", 'int': '5', 'tuple': '(1, 2)',
                'float': '1.25', 'set': '{1, 2}'}[kind]
        out.append(f'o = {c}({init})')
        uses = ['list(o)', 'len(o)', 'bool(o)', 'str(o)', 'repr(o)', '[o]', 'o == o', f'o == {init}', f'{init} == o',
                'sorted(o)' if kind not in ('int', 'float') else 'abs(o)', 'tuple(o)' if kind not in ('int', 'float') else 'int(o)',
                '1 in o' if kind not in ('int', 'float', 'str') else "'a' in o" if kind == 'str' else '-o', 'f"{o}"',
                'f"{o!r:>12}"', 'format(o, "")', '"%s" % (o,)', 'type(o + o).__name__', 'max(o)' if kind not in ('int', 'float') else 'float(o)',
                'hash(o) == hash(o)', 'o.copy()' if kind in ('list', 'dict', 'set') else 'o * 2', 'isinstance(o, ' + kind + ')',
                '[*o]' if kind not in ('int', 'float') else 'divmod(o, 2)', 'dict(o)' if kind == 'dict' else 'o.__class__.__name__',
                '{**o}' if kind == 'dict' else '"x".join(map(str, o))' if kind not in ('int', 'float') else 'range(o)' if kind == 'int' else 'round(o)',
                'json.dumps(o)', 'o[0]' if kind not in ('int', 'float', 'set') else 'o + 1', 'sorted(set(o))' if kind not in ('int', 'float') else 'o < 1',
                '", ".join(o)' if kind == 'str' else '[1, 2][o]' if kind == 'int' else 'o.items()' if kind == 'dict' else 'reversed(o)' if kind in ('list', 'tuple') else 'math.floor(o)' if kind == 'float' else 'o | {3}',
                'str.upper(o)' if kind == 'str' else 'hex(o)' if kind == 'int' else 'list.__len__(o)' if kind == 'list' else 'dict.get(o, "a")' if kind == 'dict' else 'o - {1}' if kind == 'set' else 'o.real',
                'copy.copy(o)', 'type(copy.deepcopy(o)).__name__', 'o.upper()' if kind == 'str' else 'o.bit_length()' if kind == 'int' else 'o.count(1)' if kind in ('list', 'tuple') else 'o.get("a")' if kind == 'dict' else 'o.is_integer()' if kind == 'float' else 'o.union([5])']
        return ['import json, copy'] + out + self.probes(self.r.sample(uses, self.r.randint(5, 12)))

    def b_dispatch(self):
        """Binary operators between two classes whose dunders return a tag
        or NotImplemented: reflected operands, subclass priority, in-place
        fallback and comparison mirroring."""
        ops = [('+', 'add'), ('-', 'sub'), ('*', 'mul'), ('/', 'truediv'), ('//', 'floordiv'), ('%', 'mod'),
               ('**', 'pow'), ('<<', 'lshift'), ('&', 'and'), ('|', 'or'), ('^', 'xor'), ('@', 'matmul')]
        cmps = [('<', 'lt'), ('<=', 'le'), ('==', 'eq'), ('!=', 'ne'), ('>', 'gt'), ('>=', 'ge')]
        picked = self.r.sample(ops, self.r.randint(1, 3))
        pcmp = self.r.sample(cmps, self.r.randint(1, 3))
        a, b = self.fresh('A'), self.fresh('B')
        out = []
        sub = self.chance(0.5)
        for cls, base in ((a, None), (b, a if sub else None)):
            out.append(f'class {cls}({base}):' if base else f'class {cls}:')
            out += ['    def __repr__(self):', f'        return {cls!r}']
            for _, name in picked:
                for pre in ('', 'r', 'i'):
                    if self.chance(0.5):
                        ret = 'NotImplemented' if self.chance(0.4) else f'({cls!r} + ".{pre}{name}", type(other).__name__)'
                        out += [f'    def __{pre}{name}__(self, other):', f'        return {ret}']
            for _, name in pcmp:
                if self.chance(0.6):
                    ret = 'NotImplemented' if self.chance(0.4) else f'{cls!r} + ".{name}"'
                    out += [f'    def __{name}__(self, other):', f'        return {ret}']
        out += [f'x, y = {a}(), {b}()']
        exprs = []
        for sym, _ in picked:
            for l, r in [('x', 'y'), ('y', 'x'), ('x', '1'), ('1', 'y'), ('x', 'x')]:
                exprs.append(f'{l} {sym} {r}')
            exprs.append(f'(lambda t: (t.__i{dict(ops)[sym]}__(y) if hasattr(t, "__i{dict(ops)[sym]}__") else "no i"))(x)')
        for sym, _ in pcmp:
            for l, r in [('x', 'y'), ('y', 'x'), ('x', '0'), ('None', 'y')]:
                exprs.append(f'{l} {sym} {r}')
        out += self.probes(exprs)
        for sym, _ in picked:
            if sym != '@' or self.chance(0.5):
                out += ['z = x', 'try:', f'    z {sym}= y', '    print("inplace", z)', 'except Exception as e:',
                        '    print(type(e).__name__, e)']
        return out

    def b_class_protocols(self):
        c = self.fresh('P')
        feats = self.r.sample(['getattr', 'setattr', 'descriptor', 'getitem_iter', 'call', 'format', 'index', 'bool_len',
                               'contains_iter', 'property_set', 'class_getitem', 'eq_nohash', 'mro', 'abstract', 'hash_none',
                               'init_subclass', 'set_name'], self.r.randint(1, 4))
        out = []
        uses = []
        if 'descriptor' in feats or 'set_name' in feats:
            out += ['class Desc:', '    def __set_name__(self, owner, name):', '        self.name = "_" + name',
                    '    def __get__(self, obj, objtype=None):', '        return ("get", obj is None) if obj is None else getattr(obj, self.name, "unset")',
                    '    def __set__(self, obj, value):', '        setattr(obj, self.name, value * 2)']
        if 'mro' in feats:
            out += ['class M0:', '    def who(self):', '        return ["M0"]', 'class M1(M0):', '    def who(self):',
                    '        return ["M1"] + super().who()', 'class M2(M0):', '    def who(self):', '        return ["M2"] + super().who()']
        bases = 'M1, M2' if 'mro' in feats else ''
        if 'abstract' in feats:
            out.insert(0, 'import abc')
            bases = (bases + ', ' if bases else '') + 'abc.ABC'
        if 'init_subclass' in feats:
            out += ['class Reg:', '    seen = []', '    def __init_subclass__(cls):', '        Reg.seen.append(cls.__name__)']
            bases = (bases + ', ' if bases else '') + 'Reg'
        out.append(f'class {c}({bases}):' if bases else f'class {c}:')
        out += ['    def __init__(self, v):', '        self.v = v']
        if 'getattr' in feats:
            out += ['    def __getattr__(self, name):', '        if name.startswith("x"):', '            return name * 2',
                    '        raise AttributeError(name)']
            uses += ['o.xy', 'o.nope', 'getattr(o, "xz", 0)', 'hasattr(o, "q")', 'o.v']
        if 'setattr' in feats:
            out += ['    def __setattr__(self, name, value):', '        object.__setattr__(self, name, (name, value))',
                    '    def __delattr__(self, name):', '        print("del", name)']
            uses += ['o.v', '(setattr(o, "w", 3), o.w)[1]', '(delattr(o, "v"), o.v)[1]', 'vars(o)']
        if 'descriptor' in feats or 'set_name' in feats:
            out += ['    d = Desc()']
            uses += ['o.d', f'{c}.d', '(setattr(o, "d", 4), o.d, o._d)', 'vars(o)']
        if 'getitem_iter' in feats:
            out += ['    def __getitem__(self, i):', '        if i >= 3:', '            raise IndexError(i)', '        return i * 10']
            uses += ['list(o)', '20 in o', '5 in o', 'o[1]', 'sorted(o)', '[*o]', 'list(reversed(o))', 'iter(o).__class__.__name__']
        if 'call' in feats:
            out += ['    def __call__(self, *a, **k):', '        return (a, sorted(k.items()))']
            uses += ['o(1, k=2)', 'callable(o)', 'list(map(o, [1, 2]))']
        if 'format' in feats:
            out += ['    def __format__(self, spec):', '        return "F[" + spec + "]"']
            uses += ['f"{o:>5}"', 'format(o)', '"{:x}".format(o)', 'f"{o!s}"' if 'getattr' not in feats else 'f"{o!r}"[:3]']
        if 'index' in feats:
            out += ['    def __index__(self):', '        return 2']
            uses += ['[10, 20, 30][o]', 'hex(o)', 'range(o)', '"abc"[o:]', 'bin(o)', '[1] * o', 'operator.index(o)', 'int(o)', 'o + 1']
        if 'bool_len' in feats:
            out += ['    def __len__(self):', f'        return {self.pick(["0", "2", "-1", "2 ** 70", "True"])}']
            uses += ['bool(o)', 'len(o)', 'not o', '"y" if o else "n"']
        if 'contains_iter' in feats:
            out += ['    def __iter__(self):', '        yield from (1, 2, 3)']
            uses += ['2 in o', '7 in o', 'list(o)', 'sum(o)', 'dict.fromkeys(o)', 'sorted(o, reverse=True)']
        if 'property_set' in feats:
            out += ['    @property', '    def p(self):', '        return self.v', '    @p.setter', '    def p(self, value):',
                    '        self.v = value + 1', '    @p.deleter', '    def p(self):', '        print("deleting")']
            uses += ['o.p', '(setattr(o, "p", 5), o.p)[1]', '(delattr(o, "p"), 0)[1]', f'type({c}.p).__name__']
        if 'class_getitem' in feats:
            out += ['    def __class_getitem__(cls, item):', '        return (cls.__name__, item)']
            uses += [f'{c}[int]', f'{c}[1, 2]']
        if 'eq_nohash' in feats:
            out += ['    def __eq__(self, other):', '        return True']
            uses += ['hash(o)', '{o}', 'o == 1', 'o != 1', '[o].count(5)', f'{c}.__hash__']
        if 'hash_none' in feats:
            out += ['    __hash__ = None']
            uses += ['hash(o)', '{o: 1}', 'isinstance(o, collections.abc.Hashable)']
        if 'abstract' in feats:
            out += ['    @abc.abstractmethod', '    def must(self):', '        ...']
            uses += [f'{c}(1)', f'type("{c}2", ({c},), {{"must": lambda self: 1}})(1).must()']
        if 'mro' in feats:
            uses += ['o.who()', f'[k.__name__ for k in {c}.__mro__]', f'{c}.__bases__']
        if 'init_subclass' in feats:
            uses += ['Reg.seen', f'type("Late", ({c},), {{}}).__name__', 'Reg.seen']
        uses += ['type(o).__name__', f'isinstance(o, {c})', f'{c}.__qualname__', 'o.v']
        out += ['import operator, collections.abc', 'try:', f'    o = {c}({self.expr("int")})', 'except Exception as e:',
                '    print(type(e).__name__, e)', f'    o = {c}.__new__({c})', '    object.__setattr__(o, "v", 0)']
        return out + self.probes(uses)

    def b_generator_protocol(self):
        g = self.fresh('co')
        ign = self.chance(0.3)
        out = [f'def {g}(limit):', '    total = 0', '    try:', '        while True:', '            x = yield total',
               '            if x is None or total > limit:', '                break', '            total += x',
               '    except ValueError as e:', '        yield ("caught", str(e))']
        if ign:
            out += ['    except GeneratorExit:', '        print("exit")', '        raise']
        out += ['    finally:', '        print("fin", total)', '    return total']
        out += [f'def outer{g}():', f'    r = yield from {g}({self.r.randint(0, 20)})', '    yield ("result", r)']
        steps = []
        for _ in range(self.r.randint(2, 6)):
            steps.append(self.pick([f'it.send({self.r.randint(-3, 9)})', 'next(it)', 'it.throw(ValueError("v"))',
                                    'it.throw(KeyError("k"))', 'it.close()', 'it.send(None)', 'it.throw(ValueError)',
                                    'it.gi_running']))
        out.append(f'for it in [{g}({self.r.randint(0, 20)}), outer{g}()]:')
        out.append('    print(next(it))')
        # Closed at the end: mm has no finalizers (docs/spec.md §9.2), so a
        # generator dropped unfinished never runs its `finally`.
        for s in steps + ['it.close()']:
            out += ['    try:', f'        print({s!r}, {s})', '    except StopIteration as e:', '        print("stop", e.value)',
                    '    except Exception as e:', '        print(type(e).__name__, e)']
        return out

    def b_args(self):
        f = self.fresh('fn')
        params = self.pick(['a, b=2, /, c=3, *args, d, e=5, **kw', 'a, /, b, *, c', 'a, b=1, *args', '*, a, b=2',
                            'a, b, /', 'a=1, /, *args, **kw', 'a, *args, b=0, **kw', 'a, b=[], c=None'])
        out = [f'def {f}({params}):']
        names = [p.strip().split('=')[0].lstrip('*') for p in params.split(',') if p.strip() not in ('/', '*')]
        out.append(f'    return ({", ".join(names)},)')
        calls = []
        for _ in range(self.r.randint(3, 8)):
            pos = [str(self.r.randint(0, 9)) for _ in range(self.r.randint(0, 5))]
            kws = self.r.sample(['a', 'b', 'c', 'd', 'e', 'z'], self.r.randint(0, 3))
            parts = pos + [f'{k}={self.r.randint(0, 9)}' for k in kws]
            if self.chance(0.2):
                parts.append('*[7, 8]')
            if self.chance(0.2):
                parts.append('**{' + ', '.join(f"{k!r}: 1" for k in self.r.sample(['a', 'b', 'c', 'd', 'z'], self.r.randint(0, 2))) + '}')
            calls.append(f'{f}({", ".join(parts)})')
        builtins = ['len()', 'len(1, 2)', 'int(x=1)', 'sorted([1], [2])', 'print(sep=1)', 'max()', 'min([])',
                    'range("a")', 'divmod(1)', 'round(1.5, "x")', '"".join(1)', 'sorted([3, 1], key=1)', 'list(1)',
                    'dict(1)', 'dict([1])', 'dict([(1, 2, 3)])', 'zip(1)', 'map(len)', 'enumerate()', 'isinstance(1)',
                    'isinstance(1, 1)', 'getattr(1)', 'sum([1], start="")', 'sum(["a"], "")', 'str(1, 2)',
                    'int("1", 2, 3)', 'pow(1, 2, 3, 4)', 'abs()', 'iter()', 'next()', '"a".split(x=1)',
                    '[].sort(1)', '[].sort(reverse=None)', 'sorted([], reverse="x")', 'max([1], key=None, x=1)',
                    '(lambda: 0)(1)', '(lambda a: 0)()', '(lambda *, a: 0)(1)', 'dict.fromkeys()', '"{}".format()',
                    '"{0}{1}".format(1)', '"{a}".format(b=1)', '"%s %s" % (1,)', '"%s" % (1, 2)', '"%(a)s" % 1',
                    'int.from_bytes()', '"x".encode(1)', '[].index()', 'tuple(1, 2)', 'set(1, 2)', 'frozenset([[]])',
                    'type("X")', 'type("X", 1, {})', 'object(1)', 'super()', 'reversed()', 'slice()', 'filter(None)',
                    'any()', 'chr()', 'ord("ab")', 'hash([])', 'bytes("x")', 'float("1", 2)', 'complex("1+")',
                    'bool(1, 2)', 'callable()', 'hex(1.0)', 'format(1, 2)', 'vars(1)', 'dir(1, 2)', 'id()']
        calls += self.r.sample(builtins, self.r.randint(2, 6))
        return out + self.probes(calls)

    def b_exceptions(self):
        excs = ['ValueError(1)', 'TypeError("t")', 'KeyError("k")', 'IndexError()', 'ZeroDivisionError("z")',
                'OSError(2, "nf")', 'RuntimeError("r", 2)', 'StopIteration(5)', 'ArithmeticError()', 'LookupError("l")',
                'NotImplementedError("n")', 'AttributeError("a")', 'UnicodeError("u")', 'OverflowError()', 'NameError("x")']
        k = self.r.randint(0, 3)
        if k == 0:
            group = ', '.join(self.r.sample(excs, self.r.randint(1, 4)))
            if self.chance(0.3):
                group += f', ExceptionGroup("inner", [{self.pick(excs)}, {self.pick(excs)}])'
            clauses = self.r.sample(['ValueError', '(TypeError, KeyError)', 'LookupError', 'ArithmeticError', 'OSError',
                                     'Exception', 'RuntimeError', 'ExceptionGroup'], self.r.randint(1, 3))
            out = ['try:', '    try:', f'        raise ExceptionGroup("eg", [{group}])']
            for cl in clauses:
                out += [f'    except* {cl} as g:', f'        print({cl!r}, repr(g), [repr(e) for e in g.exceptions], g.message)']
                if self.chance(0.2):
                    out += ['        raise ValueError("from star")']
            out += ['except BaseException as e:', '    print("rest", repr(e), type(e).__name__)']
            out += [f'eg = ExceptionGroup("m", [{group}])', 'print(repr(eg), str(eg), len(eg.exceptions))',
                    f'print(eg.split({self.pick(clauses[:1] + ["lambda e: isinstance(e, ValueError)"])}), eg.subgroup(lambda e: False))',
                    f'print(repr(eg.derive([ValueError(9)])), BaseExceptionGroup("b", [ValueError()]).__class__.__name__)']
            return out
        if k == 1:
            return ['def chain():', '    try:', f'        raise {self.pick(excs)}', '    except Exception as e:',
                    f'        {self.pick(["raise KeyError(\"second\")", "raise KeyError(\"none\") from None", "raise", "raise TypeError(\"cause\") from e", "e.add_note(\"note1\"); raise", "return e"])}',
                    'try:', '    r = chain()', '    print("returned", repr(r), r.__context__, r.__traceback__ is not None)',
                    'except Exception as e:', '    print(repr(e), repr(e.__context__), repr(e.__cause__), e.__suppress_context__, getattr(e, "__notes__", None))']
        if k == 2:
            exprs = [f'repr({e})' for e in self.r.sample(excs, 3)] + [f'str({e})' for e in self.r.sample(excs, 3)] + \
                    [f'{e}.args' for e in self.r.sample(excs, 2)] + \
                    ['str(KeyError())', 'str(KeyError("a", "b"))', 'repr(KeyError(""))', 'OSError(2, "x").errno',
                     'str(OSError(2, "x", "f"))', 'b"\\xff".decode()', '"\\u00e9".encode("ascii")', 'str(SyntaxError("s"))',
                     'StopIteration(1, 2).value', 'repr(BaseException())', 'repr(SystemExit(3))', 'SystemExit(3).code',
                     'Exception("a").with_traceback(None).args', 'str(UnicodeDecodeError("utf-8", b"ab", 0, 1, "bad"))',
                     'repr(ExceptionGroup("x", [ValueError()]))', 'isinstance(ExceptionGroup("x", [ValueError()]), Exception)',
                     'ExceptionGroup("x", [])', 'ExceptionGroup("x", [BaseException()])', 'BaseExceptionGroup("x", [ValueError()])',
                     'issubclass(KeyError, LookupError)', 'ZeroDivisionError.__mro__', 'str(AttributeError("x", name="n", obj=1))',
                     'AttributeError("x", name="n").name', 'NameError("x", name="y").name', 'repr(ImportError("m", name="n"))',
                     'ModuleNotFoundError("m").msg', 'str(StopAsyncIteration())', 'repr(Warning("w"))', 'TimeoutError.__mro__[1]',
                     'repr(FileNotFoundError(2, "No such"))', 'str(FileNotFoundError(2, "No such", "f.txt"))',
                     'OSError(13, "x").__class__.__name__', 'OSError(99, "x").__class__.__name__']
            return self.probes(self.r.sample(exprs, self.r.randint(5, 12)))
        return ['def f(n):', '    try:', '        if n == 0:', f'            raise {self.pick(excs)}', '        return f(n - 1)',
                '    finally:', f'        {self.pick(["pass", "print(\"unwind\", n)", "n = -1"])}',
                'for n in range(3):', '    try:', '        print(f(n))', '    except Exception as e:',
                '        print(type(e).__name__, e)', '    else:', '        print("else", n)', '    finally:', '        print("done", n)',
                'def ret():', '    try:', '        return "try"', '    finally:', f'        {self.pick(["print(\"fin\")", "pass"])}',
                'print(ret())', 'try:', '    try:', '        1 / 0', '    finally:', '        print("inner fin")',
                f'except {self.pick(["ZeroDivisionError", "ArithmeticError", "Exception"])} as e:', '    print("outer", e)',
                'print("end")']

    def b_iterstd(self):
        L, S = (lambda: self.expr('list')), (lambda: self.expr('str'))
        n = lambda: self.r.randint(-1, 5)
        exprs = [self.pick([
            f'list(itertools.accumulate({L()}{self.pick(["", ", operator.mul", ", max", ", initial=10"])}))',
            f'list(itertools.chain({L()}, {S()}))', f'list(itertools.chain.from_iterable([{L()}, {S()}]))',
            f'list(itertools.islice({L()}, {n()}, {n()}, {self.pick(["1", "2", "None", "0", "-1"])}))',
            f'list(itertools.product({S()}[:3], repeat={self.r.randint(0, 2)}))', f'list(itertools.permutations({L()}[:4], {n()}))',
            f'list(itertools.combinations({L()}, {n()}))', f'list(itertools.combinations_with_replacement({S()}[:3], {n()}))',
            f'[(k, list(g)) for k, g in itertools.groupby({S()})]', f'[(k, list(g)) for k, g in itertools.groupby(sorted({L()}), key=lambda q: q % 3)]',
            f'list(itertools.zip_longest({L()}, {S()}, fillvalue="-"))', f'list(itertools.starmap(pow, zip({L()}, [2, 3, 0])))',
            f'list(itertools.takewhile(lambda q: q < 5, {L()}))', f'list(itertools.dropwhile(lambda q: q < 5, {L()}))',
            f'list(itertools.pairwise({S()}))', f'list(itertools.batched({L()}, {n()}))', f'list(itertools.compress({S()}, {L()}))',
            f'list(itertools.islice(itertools.count({n()}, {self.pick(["1", "-2", "0.5"])}), 5))', f'list(itertools.islice(itertools.cycle({S()}), 7))',
            f'list(itertools.repeat({S()}, {n()}))', f'list(itertools.filterfalse(None, {L()}))', f'[list(t) for t in itertools.tee({L()}, 2)]',
            f'functools.reduce(lambda a, b: a * 2 + b, {L()}{self.pick(["", ", 1"])})', f'functools.partial(pow, 2)({n()})',
            f'functools.partial(int, base=2)({self.pick(["\"101\"", "\"2\"", "\"0b11\""])})', f'sorted({L()}, key=functools.cmp_to_key(lambda a, b: (a % 3) - (b % 3)))',
            f'list(map(operator.itemgetter({self.pick(["0", "1, 0", "-1"])}), [{L()}, (1, 2), "abc"]))', f'operator.attrgetter("real", "imag")({self.expr("int")})',
            f'operator.methodcaller("split", ",")({S()})', f'heapq.nlargest({n()}, {L()})', f'heapq.nsmallest({n()}, {S()}, key=str.lower)',
            f'list(heapq.merge(sorted({L()}), sorted({L()}), reverse=False))', f'bisect.bisect_left(sorted({L()}), {self.expr("int")})',
            f'bisect.bisect_right(sorted({L()}), {self.expr("int")}, {n()})', f'(lambda h: (bisect.insort(h, 4), h)[1])(sorted({L()}))',
            f'(lambda h: (heapq.heapify(h), heapq.heappop(h), heapq.heappushpop(h, 3), h)[1:])({L()})',
            f'statistics.median({L()})', f'statistics.mode({S()})', f'statistics.mean({L()})', f'statistics.multimode({L()})',
            f'list(zip(*[{S()}, {S()}], strict=False))', f'list(map(pow, {L()}, {L()}))', f'list(iter([3, 2, 1, 0, 5].pop, 0))',
            f'sorted({L()}, key=lambda q: -q, reverse=True)', f'copy.deepcopy([{L()}, {{"a": {L()}}}])',
            f'sum({self.pick(["[0.1] * 10", "[1, 2.5]", "[1e100, 1, -1e100]", "[]", "[True, True]"])})',
            f'math.prod({L()})', f'math.dist([{n()}, {n()}], [{n()}, {n()}])', f'math.sumprod({L()}, {L()})'])
            for _ in range(self.r.randint(4, 9))]
        return ['import itertools, functools, operator, heapq, bisect, statistics, copy'] + self.probes(exprs)

    def b_lru(self):
        size = self.pick(['None', '2', '0', '128'])
        n = self.r.randint(0, 60 if size in ('None', '128') else 18)
        return ['import functools', f'@functools.lru_cache(maxsize={size})',
                'def fib(n):', '    return n if n < 2 else fib(n - 1) + fib(n - 2)', f'print(fib({n}), fib.cache_info())',
                'fib.cache_clear()', 'print(fib.cache_info(), fib.__name__, fib.__wrapped__(1))',
                'def deco(f):', '    @functools.wraps(f)', '    def w(*a):', '        return ("w", f(*a))', '    return w',
                '@deco', 'def named(x):', '    "doc"', '    return x', 'print(named(3), named.__name__, named.__doc__, named.__wrapped__(1))',
                '@functools.total_ordering', 'class V:', '    def __init__(self, v):', '        self.v = v',
                '    def __eq__(self, o):', '        return self.v == o.v', '    def __lt__(self, o):', '        return self.v < o.v',
                f'print(V(1) >= V(2), V(3) > V(2), V(2) <= V(2), sorted([V(3), V(1)], reverse=True)[0].v)',
                '@functools.singledispatch', 'def kind(x):', '    return "obj"', '@kind.register(int)', 'def _(x):', '    return "int"',
                '@kind.register(str)', 'def _(x):', '    return "str"', f'print(kind(1), kind("a"), kind(True), kind([]), kind(2.5))',
                'print(functools.cache(lambda q: q * 2)(4))']

    def b_records(self):
        k = self.r.randint(0, 1)
        if k == 0:
            order, frozen = self.pick(['True', 'False']), self.pick(['True', 'False'])
            return ['import dataclasses', f'@dataclasses.dataclass(order={order}, frozen={frozen})', 'class Pt:',
                    '    x: int', '    y: str = "a"', '    z: list = dataclasses.field(default_factory=list, compare=False)',
                    '    w: int = dataclasses.field(default=0, repr=False)',
                    f'p, q = Pt({self.expr("int")}), Pt({self.expr("int")}, {self.expr("str")}, [1])',
                    'print(p, q, p == q, dataclasses.asdict(q), dataclasses.astuple(p), [f.name for f in dataclasses.fields(Pt)])',
                    'print(dataclasses.replace(p, x=9), dataclasses.is_dataclass(p), Pt.__match_args__)'] + self.probes(
                    ['p < q', 'sorted([q, p])', 'hash(p) == hash(Pt(p.x))', '(setattr(p, "x", 5), p)[1]', 'Pt()', 'Pt(1, 2, 3, 4, 5)',
                     'Pt(1, z=None)', 'dataclasses.replace(p, nope=1)', 'Pt.__eq__(p, 1)'])
        flag = self.chance(0.5)
        if flag:
            return ['import enum', 'class Perm(enum.Flag):', '    R = 4', '    W = 2', '    X = 1', '    RW = 6'] + self.probes(
                ['Perm.R | Perm.W', 'repr(Perm.R | Perm.X)', 'Perm(7)', 'Perm(0)', 'list(Perm)', 'Perm.RW.name', '~Perm.R',
                 'Perm.R in Perm.RW', 'Perm(8)', 'str(Perm.W | Perm.X)', 'len(Perm.RW)', 'list(Perm.RW)', 'bool(Perm(0))',
                 'Perm["R"]', 'format(Perm.R)', 'Perm.R & Perm.W', 'Perm.R ^ Perm.RW', 'repr(Perm(0))'])
        base = self.pick(['enum.Enum', 'enum.IntEnum', 'enum.StrEnum'])
        vals = ['enum.auto()', 'enum.auto()', 'enum.auto()'] if base != 'enum.Enum' or self.chance(0.4) else ['1', '"two"', '1']
        return ['import enum', f'class Color({base}):', f'    RED = {vals[0]}', f'    GREEN = {vals[1]}', f'    BLUE = {vals[2]}',
                '    def describe(self):', '        return self.name.lower()'] + self.probes(
            ['Color.RED', 'repr(Color.GREEN)', 'Color(1)', 'Color["BLUE"]', 'list(Color)', 'Color.RED.value', 'Color(99)',
             'Color.BLUE.describe()', 'Color.RED == 1', 'Color.RED < Color.GREEN', 'f"{Color.RED}"', 'str(Color.RED)', 'len(Color)',
             'Color.BLUE is Color.RED', 'Color.__members__.keys()', 'hash(Color.RED) == hash(Color.RED)', 'Color.RED + 1',
             'sorted(Color, key=lambda c: c.name)', 'isinstance(Color.RED, Color)', 'Color("red")', 'Color.RED.name', 'format(Color.GREEN, ">8")'])

    def b_json(self):
        docs = [self.expr('dict'), self.expr('list'), self.expr('slist'), '{"a": [1, 2.5, None, True], "b": {"c": "\\u00e9"}}',
                '[1e100, -0.0, 0.1, 10 ** 20, float("inf")]', '{1: "int key", None: 0, True: 1, 2.5: "f"}', '"tab\\there"']
        texts = ['"[1, 2"', '"{\\"a\\": 1,}"', '"[1, 2] x"', '""', '"nul"', '"{\\"a\\" 1}"', '"[01]"', '"1e400"', '"-0"',
                 '"\\"\\\\u00e9\\""', '"[1,\\n 2,\\n ]"', '"NaN"', '" {} "', '"{\\"a\\": [true, false, null]}"', '"1.5e3"',
                 '"\\"\\\\x\\""', '"[\\"a\\"\\n"', '"123abc"', '"{\\"k\\":1, \\"k\\":2}"', '"[1e-400]"', '"\\"\\\\ud83d\\\\ude00\\""']
        exprs = []
        for _ in range(self.r.randint(3, 8)):
            d = self.pick(docs)
            exprs.append(self.pick([
                f'json.dumps({d})', f'json.dumps({d}, sort_keys=True, indent={self.pick(["None", "0", "2", "\"\\t\""])})',
                f'json.dumps({d}, separators=(",", ":"), ensure_ascii={self.pick(["True", "False"])})',
                f'json.loads(json.dumps({d}))', f'json.loads({self.pick(texts)})', f'json.dumps({d}, allow_nan=False)',
                f'json.loads({self.pick(texts)}, parse_float=str, parse_int=float)', f'json.dumps({{"k": {{1, 2}}}})',
                f'json.dumps({d}, default=str, skipkeys=True)', f'json.loads({self.pick(texts)}, object_pairs_hook=list)']))
        return ['import json'] + self.probes(exprs)

    def b_regex(self):
        pats = [r'\d+', r'(\w+)@(\w+)\.com', r'[a-z]+', r'^\s*(\w)', r'(a|b)*c', r'(?P<k>\w)=(?P<v>\d)', r'\bh\w*',
                r'(?i)HELLO', r'x*', r'[^,]+', r'(\d)(?=\d)', r'(.)\1', r'a{2,3}', r'\W', r'[\u00e0-\u00ff]', r'(?:ab)+?',
                r'(', r'[a-', r'a**', r'(?<=a)b', r'$', r'(?P<n>x)(?P=n)', r'\s+|,', r'[a\]-]', r'.', r'(?s).+', r'\Z']
        strs = ['"a1b22c333"', '"joe@site.com, ann@web.com"', '"hello world Hello"', '"  x = 1, y = 2"', '"aabbcc"', '"caf\\u00e9"',
                '""', '"a,b,,c"', '"aaa ab abab"', '"k=1 v=2"', '"x\\ny"']
        exprs = []
        for _ in range(self.r.randint(3, 8)):
            p, s = repr(self.pick(pats)), self.pick(strs)
            exprs.append(self.pick([
                f're.findall({p}, {s})', f're.sub({p}, "<\\\\g<0>>", {s})', f're.split({p}, {s}, maxsplit={self.r.randint(0, 3)})',
                f'(lambda m: m and (m.group(), m.groups(), m.span(), m.groupdict()))(re.search({p}, {s}))',
                f'bool(re.fullmatch({p}, {s}))', f'[m.span() for m in re.finditer({p}, {s})]', f're.subn({p}, lambda m: m.group().upper(), {s})',
                f're.compile({p}).pattern', f're.escape({s})', f'(lambda m: m and m.group(0))(re.match({p}, {s}, re.IGNORECASE))',
                f're.sub({p}, r"\\1", {s})']))
        return ['import re'] + self.probes(exprs)

    def b_asyncio(self):
        k = self.r.randint(0, 4)
        n1, n2 = self.r.randint(0, 4), self.r.randint(0, 4)
        fail = self.pick(['None', '1', '2'])
        head = ['import asyncio', 'log = []',
                'async def worker(name, n, fail=None):', '    for i in range(n):',
                f'        await asyncio.sleep({self.pick(["0", "0.001 * i"])})', '        log.append((name, i))',
                '        if i == fail:', '            raise ValueError(name)', '    return name * n']
        if k == 0:
            body = ['async def main():', f'    r = await asyncio.gather(worker("a", {n1}), worker("b", {n2}, {fail}), return_exceptions={self.pick(["True", "False"])})',
                    '    return r']
        elif k == 1:
            body = ['async def main():', '    try:', '        async with asyncio.TaskGroup() as tg:',
                    f'            t1 = tg.create_task(worker("a", {n1}, {self.pick(["None", "0", "1"])}))', f'            t2 = tg.create_task(worker("b", {n2}, {fail}))',
                    '        return t1.result(), t2.result()', '    except* ValueError as eg:', '        got = ("eg", [str(e) for e in eg.exceptions])',
                    '    return got']
        elif k == 2:
            body = ['async def main():', f'    q = asyncio.Queue(maxsize={self.r.randint(0, 3)})',
                    '    async def prod():', f'        for i in range({n1 + 2}):', '            await q.put(i)', '            log.append(("put", i))',
                    '        await q.put(None)', '    async def cons():', '        out = []', '        while (item := await q.get()) is not None:',
                    '            out.append(item)', '            log.append(("got", item))', '        return out',
                    '    p = asyncio.create_task(prod())', '    c = asyncio.create_task(cons())', '    await p', '    return await c, q.qsize(), q.empty()']
        elif k == 3:
            body = ['async def main():', f'    t = asyncio.create_task(worker("a", {n1 + 2}))', f'    for _ in range({self.r.randint(0, 3)}):',
                    '        await asyncio.sleep(0)', f'    t.cancel({self.pick(["", "\"msg\""])})', '    try:', '        await t',
                    '    except asyncio.CancelledError as e:', '        log.append(("cancelled", e.args))',
                    '    return t.cancelled(), t.done()']
        else:
            body = ['async def main():', '    try:', f'        return await asyncio.wait_for(worker("a", {n1 + 1}), timeout={self.pick(["0", "1"])})',
                    '    except TimeoutError as e:', '        return ("timeout", type(e).__name__)',
                    '    finally:', '        ev = asyncio.Event()', '        asyncio.get_running_loop().call_soon(ev.set)', '        await ev.wait()',
                    '        log.append(("event", ev.is_set()))']
        return head + body + ['print(asyncio.run(main()), log)']

    def b_metaclass(self):
        """A metaclass's `__new__`, `__init__`, `__call__` and class-level
        protocols (`len(C)`, `C[k]`, `x in C`, `isinstance` hooks), class
        keywords, `type(name, bases, ns)` and metaclass conflicts. Without
        `__prepare__`, `__slots__` or `__init_subclass__`: the first two are
        refused by design (docs/user-guide.md), the last belongs to the
        types lane."""
        m, c = self.fresh('Meta'), self.fresh('K')
        out = [f'class {m}(type):', '    made = []']
        if self.chance(0.7):
            out += ['    def __new__(mcls, name, bases, ns, **kw):', '        ns.setdefault("tag", name.lower())',
                    '        cls = super().__new__(mcls, name, bases, ns)', '        mcls.made.append((name, sorted(kw)))',
                    '        return cls']
        if self.chance(0.5):
            out += ['    def __init__(cls, name, bases, ns, **kw):', '        super().__init__(name, bases, ns)',
                    '        cls.kw = kw']
        if self.chance(0.5):
            out += ['    def __call__(cls, *a, **kw):', '        obj = super().__call__(*a, **kw)',
                    f'        obj.via = {self.pick(["cls.__name__", "len(a)", "sorted(kw)"])}', '        return obj']
        if self.chance(0.5):
            out += ['    def __len__(cls):', f'        return {self.pick(["3", "len(cls.__name__)", "-1", "2 ** 70", "True"])}',
                    '    def __getitem__(cls, k):', '        return (cls.__name__, k)',
                    '    def __iter__(cls):', '        return iter(cls.__name__)',
                    '    def __contains__(cls, x):', '        return x == cls.__name__']
        if self.chance(0.4):
            out += ['    def __instancecheck__(cls, x):', f'        return {self.pick(["isinstance(x, int)", "True", "False", "x == 1"])}',
                    '    def __subclasscheck__(cls, s):', f'        return {self.pick(["s is bool", "True", "issubclass(s, int)"])}']
        if self.chance(0.4):
            out += ['    def __repr__(cls):', '        return f"<{cls.__name__} via meta>"', '    @property',
                    '    def plural(cls):', '        return cls.__name__ + "s"', '    def hello(cls):', '        return "meta hello"']
        if self.chance(0.3):
            out += ['    def __getattr__(cls, name):', '        if name.startswith("x"):', '            return name.upper()',
                    '        raise AttributeError(name)']
        kw = self.pick(['', ', flag=1', ', a=1, b="two"'])
        out += [f'class {c}(metaclass={m}{kw}):', '    def __init__(self, v=0):', '        self.v = v',
                '    def hello(self):', '        return "instance hello"', f'class Sub{c}({c}):', '    pass']
        return out + self.probes([
            f'type({c}).__name__', f'type(Sub{c}).__name__', f'{m}.made', f'getattr({c}, "kw", None)', f'{c}.tag',
            f'Sub{c}.tag', f'{c}(5).v', f'getattr({c}(1, ), "via", None)', f'{c}(v=2).via if hasattr({c}(), "via") else 0',
            f'len({c})', f'{c}[3]', f'list({c})', f'"{c}" in {c}', f'1 in {c}', f'isinstance(1, {c})', f'isinstance({c}(), {c})',
            f'issubclass(bool, {c})', f'issubclass(Sub{c}, {c})', f'repr({c})', f'str(Sub{c})', f'{c}.plural', f'{c}.hello()',
            f'{c}().hello()', f'{c}.xyz', f'{c}.nope', f'{c}.__mro__ == ({c}, object)', f'[k.__name__ for k in Sub{c}.__mro__]',
            f'isinstance({c}, {m})', f'isinstance({c}, type)', f'{m}.__mro__[1].__name__', f'{c}.__class__ is {m}',
            f'type("Dyn", ({c},), {{"w": 1}}).w', f'type("Dyn", ({c},), {{}}).tag', f'type(type("D", (), {{}})).__name__',
            'type("D", (1,), {})', 'type("D")', 'type("D", (), {}, 1)', f'{m}("Made", (), {{}}).tag',
            f'type({m}("Made", (), {{"q": 2}})()).__name__',
            f'(lambda M2: type("X", ({c}, M2("Y", (), {{}})), {{}}))(type("M2", (type,), {{}}))',
            f'sorted(vars({c}).keys() - {{"__module__", "__dict__", "__weakref__", "__doc__", "__qualname__", "__firstlineno__", "__static_attributes__"}})',
        ])

    def b_dates_typing_text(self):
        """datetime arithmetic, parsing and formatting with fixed values
        (no clock), typing's runtime objects, and textwrap."""
        k = self.r.randint(0, 2)
        if k == 0:
            d = f'datetime.date({self.r.randint(1, 9999)}, {self.r.randint(1, 12)}, {self.r.randint(1, 28)})'
            t = (f'datetime.datetime({self.r.randint(1970, 2100)}, {self.r.randint(1, 12)}, {self.r.randint(1, 28)}, '
                 f'{self.r.randint(0, 23)}, {self.r.randint(0, 59)}, {self.r.randint(0, 59)}, {self.pick(["0", "1", "999999", "500000"])})')
            td = (f'datetime.timedelta({self.pick(["days", "hours", "seconds", "microseconds", "weeks", "minutes"])}='
                  f'{self.pick(["1", "-1", "1.5", "-0.5", "86399", "10 ** 6", "1e-7", "2.675"])})')
            tz = f'datetime.timezone(datetime.timedelta(hours={self.r.randint(-23, 23)}, minutes={self.pick(["0", "30", "45"])}))'
            fmts = ['%Y-%m-%d', '%d/%m/%y %H:%M', '%A %B %j', '%a %b %d %I %p', '%U %W %w', '%%%f', '%G-W%V-%u', '%c', '%x %X', '%z %Z']
            isos = ['"2024-02-29"', '"2023-02-29"', '"2024-13-01"', '"20240102"', '"2024-W05-3"', '"2024-01-02T03:04:05"',
                    '"2024-01-02 03:04:05.123+05:30"', '"2024-01-02T25:00"', '"T03:04"', '""', '"2024-01-02T03:04:05Z"']
            return ['import datetime'] + self.probes([
                f'{d}', f'{d}.isoformat()', f'{d}.weekday()', f'{d}.isocalendar()', f'{d}.toordinal()', f'{d} + {td}',
                f'{d} - {d}', f'{t} - {t}', f'{t} + {td}', f'{t}.replace(tzinfo={tz})', f'{t}.replace(tzinfo={tz}).isoformat()',
                f'{t}.replace(tzinfo={tz}).utcoffset()', f'{t} < {t}.replace(tzinfo={tz})', f'{t} == {t}.replace(tzinfo={tz})',
                f'{t}.replace(tzinfo={tz}).astimezone(datetime.timezone.utc)', f'{t}.strftime({self.pick(fmts)!r})',
                f'{d}.strftime({self.pick(fmts)!r})', f'str({td})', f'repr({td})', f'{td}.total_seconds()', f'{td} * {self.pick(["3", "-2", "0.5", "1.1"])}',
                f'{td} / {td}', f'{td} // {self.pick(["2", "0", "7"])}', f'{td} % {td}', f'divmod({td}, {td})', f'abs({td})', f'-{td}',
                f'datetime.date.fromisoformat({self.pick(isos)})', f'datetime.datetime.fromisoformat({self.pick(isos)})',
                f'datetime.time.fromisoformat({self.pick(isos)})', f'datetime.datetime.strptime("2024-03-04 05:06", "%Y-%m-%d %H:%M")',
                f'datetime.datetime.strptime({self.pick(isos)}, "%Y-%m-%d")', f'datetime.date.max + {td}', f'datetime.date.min - {td}',
                f'datetime.datetime.combine({d}, datetime.time(1, 2, 3))', f'datetime.date.fromordinal({self.r.randint(-1, 4000000)})',
                f'datetime.time({self.r.randint(0, 25)}, {self.r.randint(0, 61)})', f'{t}.timetuple()[:6]', f'{tz}', f'repr({tz})',
                f'datetime.timedelta(days=999999999) + {td}', f'hash({td}) == hash({td})', f'format({d}, {self.pick(fmts)!r})',
                f'f"{{{t}:%H}}"', f'{t}.date() == {d}', f'sorted([{t}, {t}.replace(year=1999)])[0].year', f'{t}.ctime()',
            ])
        if k == 1:
            return ['import typing', 'T = typing.TypeVar("T")', 'class Box(typing.Generic[T]):', '    def __init__(self, v):',
                    '        self.v = v', 'class Pt(typing.NamedTuple):', '    x: int', '    y: int = 0',
                    'Row = typing.TypedDict("Row", {"id": int, "name": str})', '@typing.runtime_checkable',
                    'class Sized(typing.Protocol):', '    def __len__(self): ...'] + self.probes([
                'repr(typing.Optional[int])', 'repr(typing.Union[int, str])', 'repr(typing.Union[int, int])', 'repr(int | None)',
                'typing.get_origin(typing.List[int])', 'typing.get_args(typing.Dict[str, int])', 'repr(typing.List[int])',
                'repr(list[int])', 'list[int] == typing.List[int]', 'repr(Box[int])', 'Box[int](3).v', 'Box(1).v',
                'repr(typing.Literal[1, "a"])', 'typing.get_args(typing.Literal[1, 2])', 'typing.cast(int, "x")',
                'Pt(1)', 'Pt(1, 2)._replace(y=5)', 'Pt._fields', 'Pt(1) == (1, 0)', 'Pt._make([3, 4])', 'Pt(1)._asdict()',
                'Pt()', 'Row(id=1, name="a")', 'isinstance([], Sized)', 'isinstance(3, Sized)', 'repr(T)', 'T.__name__',
                'repr(typing.Callable[[int], str])', 'repr(typing.Tuple[int, ...])', 'typing.get_args(int | str)',
                'repr(typing.Any)', 'typing.Optional[int] == typing.Union[int, None]', 'repr(typing.Final[int])',
                'repr(typing.ClassVar[int])', 'Box[int, str]', 'typing.Generic[int]', 'repr(typing.Iterable[str])',
                'issubclass(list, typing.Iterable)', 'isinstance({}, typing.Mapping)', 'repr(typing.Annotated[int, "m"])',
                'typing.get_args(typing.Annotated[int, "m"])', 'repr(typing.Never)', 'typing.NewType("UserId", int)(5)',
                'repr(typing.NewType("UserId", int))', 'typing.assert_never(1)', 'typing.get_origin(int)',
            ])
        words = self.pick(['"The quick brown fox jumps over the lazy dog"', '"a  b\\tc\\n\\nd  e"', '"supercalifragilisticexpialidocious and more"',
                           '"  leading and trailing  "', '"line one\\nline two\\n  indented\\n"', '""', '"x " * 30', '"tab\\there, comma, dash-word"'])
        width = self.pick(['1', '5', '10', '20', '0', '-1', '80'])
        return ['import textwrap'] + self.probes([
            f'textwrap.wrap({words}, {width})', f'textwrap.fill({words}, {width})', f'textwrap.shorten({words}, {width})',
            f'textwrap.shorten({words}, width={width}, placeholder=" [..]")', f'textwrap.dedent({words})',
            f'textwrap.indent({words}, "> ")', f'textwrap.indent({words}, "+", lambda l: True)',
            f'textwrap.wrap({words}, {width}, break_long_words=False)', f'textwrap.wrap({words}, {width}, break_on_hyphens=False)',
            f'textwrap.wrap({words}, {max(int(width), 6)}, initial_indent="* ", subsequent_indent="  ")', f'textwrap.wrap({words}, {width}, max_lines=2)',
            f'textwrap.fill({words}, {width}, expand_tabs=False, replace_whitespace=False)', f'textwrap.wrap({words}, {width}, drop_whitespace=False)',
            f'textwrap.TextWrapper(width={width}, tabsize=4).wrap({words})', f'textwrap.wrap({words}, {width}, fix_sentence_endings=True)',
            'textwrap.dedent("  a\\n    b\\n  c")', 'textwrap.dedent("\\ta\\n  b")', 'textwrap.dedent("  a\\n\\n  b\\n   ")',
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


# What `mm exec --json` reports and every way of running a cell must give
# alike (docs/spec.md §13): the output, the result, the exception, the exit,
# the fuel and, with `--digest`, the final state's checksum.
PARK_KEYS = ('stdout', 'result', 'error', 'exit', 'fuel', 'digest')


def mm_json(args, cmd):
    env = dict(os.environ)
    env.pop('MM_STEP_MS', None)
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=args.timeout * 4, env=env, stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return None
    try:
        return json.loads(p.stdout)
    except ValueError:
        err = p.stderr.decode('utf-8', 'replace').strip().splitlines()
        return {'crash': p.returncode, 'stderr': err[-1] if err else ''}


def parked(args, source, seed):
    """`--park`: the cell unparked, parked in one process (a snapshot at every
    stop) and as a chain of snapshots each restored in a new process, with
    the strides of `cargo xtask durability`'s N:M policy (every N fuel units,
    or fuel / M for a longer cell). Returns the first difference from the
    unparked run, as (mode, key, unparked, parked), or None."""
    with tempfile.NamedTemporaryFile('w', suffix='.py', delete=False, dir=args.tmp) as f:
        f.write(source)
        path = f.name
    snap = tempfile.mkdtemp(dir=args.tmp)
    try:
        exe = [args.mm, 'exec', '--json', '--main-module', '--digest']
        base = mm_json(args, exe + ['--report-fuel', path])
        if base is None or 'crash' in base:
            return None
        every = random.Random(seed).choice((1, 7, 61, 1009))
        fuel = base['fuel']
        runs = [('parks', mm_json(args, exe + ['--park-every', str(max(every, -(-fuel // 200))),
                                               '--snapshot-continue', path]))]
        chain = mm_json(args, exe + ['--snapshot-every', str(max(every, -(-fuel // 25))), '--snapshot-dir', snap, path])
        for _ in range(200):
            if chain is None or 'park' not in chain:
                break
            chain = mm_json(args, [args.mm, 'resume', '--json', snap])
        runs.append(('restore', chain))
        for mode, got in runs:
            if got is None:
                return mode, 'timeout', '', 'timeout'
            for key in ('crash',) + PARK_KEYS:
                if base.get(key) != got.get(key):
                    return mode, key, base.get(key), got.get(key)
        return None
    finally:
        os.unlink(path)
        shutil.rmtree(snap, ignore_errors=True)


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
        # Both may stop at the same uncaught exception before the report.
        if not pyl or not mml or pyl[:-1] != mml[:-1] or not pyl[-1].startswith('_R ') or not mml[-1].startswith('_R '):
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
    norm = lambda t: re.sub(r'\d+', '#', re.sub(r'0x[0-9a-f]+', '#', t))[:100]
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
        found = args.park and parked(args, source, seed)
        if not found:
            return seed, 'same', None
        return park_divergence(args, seed, source, found)
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


def park_divergence(args, seed, source, found):
    """A parked run that differs from the unparked one, saved unminimized
    (the minimizer compares with CPython)."""
    mode, key, a, b = found
    norm = lambda t: re.sub(r'\d+', '#', re.sub(r'0x[0-9a-f]+', '#', str(t)))[:100]
    sig = f'park {mode}: {key}: ' + (norm(b) if key in ('crash', 'timeout') else f'{norm(a)} | {norm(b)}')
    with args.lock:
        seen = args.seen.get(sig, 0)
        args.seen[sig] = seen + 1
    if seen >= args.per_signature:
        return seed, 'diverge', (sig, None)
    head = [f'# difffuzz park divergence, seed {seed}: `{mode}` differs from the unparked run in {key}',
            '#   unparked: ' + commented(str(a)[:300]), '#   parked:   ' + commented(str(b)[:300]),
            '# signature: ' + commented(sig)]
    path = os.path.join(args.out, 'seed-%d.py' % seed)
    with open(path, 'w') as f:
        f.write('\n'.join(head) + '\n' + source)
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
    ap.add_argument('--park', action='store_true',
                    help='also compare each agreeing program parked and restored with its unparked run')
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
