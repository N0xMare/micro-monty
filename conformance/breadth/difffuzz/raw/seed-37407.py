# difffuzz divergence, seed 37407
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u206c'
#   mm:      EXC ValueError invalid literal for int() with base 2: '⁬'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c' | mm: EXC ValueError invalid literal for int() with base #: '⁬'
import math
try:
        class Ctx26:
            def __init__(self, name):
                self.name = name
            def __enter__(self):
                return self
            def __exit__(self, kind, value, tb):
                return True
        with Ctx26("a") as x, Ctx26("b"):
            raise ValueError(7)
        def f27(p0, p1):
            def f28(p0, *rest, kw=0):
                return chr(8254).replace(''[0:5].center(11, chr(45)).lstrip(('{}-{!r}-{:>4}'.format(1000, 'mississippi', i24) * (pow(-17, 32, 1000) % 4))), repr(0.1))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
        try:
            print(divmod(pow(-(10 ** 25), 33, 2 ** 61 - 1), f27('caf\u00e9', int(chr(8300), 2)))[1] // (('hello' + '3.5').zfill(8).index('{}-{!r}-{:>4}'.format('\U0001f600 ok'.rfind('Hello World'), 'Hello World'.rjust(7, chr(42)), {'k0': 1000, 'k1': -1, 'k2': 100, 'k3': 2}['k0'])) if ((divmod(1000, i30) > (i24, )) or (round(0.1) in list(reversed([])))) else (f27('  pad  '.ljust(9, chr(45)), round(0.0)) << ((math.gcd(0.1) >> (lst0[42] % 9)) % 70))))
        except KeyError as e:
            print("caught", type(e).__name__, e, e.args)
            def __init__(self, name):
                self.name = name
        def make38(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
