# difffuzz divergence, seed 7942
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u1cbb'
#   mm:      EXC ValueError invalid literal for int() with base 0: '᲻'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cbb' | mm: EXC ValueError invalid literal for int() with base #: '᲻'
import math
try:
    n6 = 5
    while n6 > 0:
        class Ctx7:
            def __init__(self, name):
                self.name = name
            def __enter__(self):
                return self
            def __exit__(self, kind, value, tb):
                return None
        with Ctx7("a") as x, Ctx7("b"):
            print(x.name * 2)
        for i8 in range(3, 8):
            def f12(p0, p1, p2):
                return '%d' % (('a'.center(6, chr(45)).replace('', '3.5'.ljust(4, chr(45))) * (min([(i8 + i14) for i14 in range(3, 7)]) % 4)), math.e,)
            print("f12(int(chr(7355), 0), str((not False)).removesuffix('x\\ny').title(), [len(w) for w in list({str(q): q for q in list(map(lambda q: q * 1 + 1, [0]))})])", f12(int(chr(7355), 0), str((not False)).removesuffix('x\ny').title(), [len(w) for w in list({str(q): q for q in list(map(lambda q: q * 1 + 1, [0]))})]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
