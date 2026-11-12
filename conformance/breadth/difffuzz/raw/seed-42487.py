# difffuzz divergence, seed 42487
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '0000000\u0acf'
#   mm:      EXC ValueError invalid literal for int() with base 10: '0000000૏'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#acf' | mm: EXC ValueError invalid literal for int() with base #: '#૏'
import math
try:
    n5 = 4
    while n5 > 0:
        class Ctx6:
            def __init__(self, name):
                self.name = name
        print('v7', v7)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f8(p0, p1):
        for i9 in range(2):
            if i9 == 0:
                continue
            *a, b = '{}-{!r}-{:>4}'.format(n5, 'hello', n5).lower().rjust(2, chr(42)).ljust(0, chr(45))
            match '%%%d' % (int(chr(2767).zfill(8), 10), chr(10387).lstrip(''),):
                case str(s) if len(s) > 3:
                    print("long", s[:3])
    print('f8([a * b for a, b in zip(([len({10, 2})] + (sorted({-3, 42, 5}) + [(100 + i10) for i10 in [10, 2]])), sorted((frozenset([-1, 100, 10]) | {9} | set([1, -3, 1, -3, 42]))))], ((sorted(sorted({255, 2, 0, 42, 0}), key=lambda q: (q % 3, -q)) * 1) * 2))', f8([a * b for a, b in zip(([len({10, 2})] + (sorted({-3, 42, 5}) + [(100 + i10) for i10 in [10, 2]])), sorted((frozenset([-1, 100, 10]) | {9} | set([1, -3, 1, -3, 42]))))], ((sorted(sorted({255, 2, 0, 42, 0}), key=lambda q: (q % 3, -q)) * 1) * 2)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
