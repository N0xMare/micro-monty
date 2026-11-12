# difffuzz divergence, seed 21984
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0eec'
#   mm:      EXC ValueError invalid literal for int() with base 16: '໬'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#eec' | mm: EXC ValueError invalid literal for int() with base #: '໬'
import math
try:
    n12 = 1
    while n12 > 0:
        def f13(p0, p1, p2, *rest, kw=1):
            try:
                print((sorted({0}) * 1) < ('Hello World'.partition(',')[2].swapcase() * (i1 % 4)).split()[0])
            except ZeroDivisionError as e:
                print("caught", type(e).__name__, e, e.args)
                def __init__(self, name):
                    self.name = name
        print("f13(int(chr(3820).strip(), 16), [len(w) for w in '{}-{!r}-{:>4}'.format((n12 << (3 % 70)), '  pad  '.rjust(0, chr(42)), (n12 // 5)).splitlines()], [i + v for i, v in enumerate([a * b for a, b in zip(([ord(c) for c in '\\U0001f600 ok'] + ([5, 255] * 3)), sorted({s17 % 4 for s17 in [1000, 100]}))], 0)])", f13(int(chr(3820).strip(), 16), [len(w) for w in '{}-{!r}-{:>4}'.format((n12 << (3 % 70)), '  pad  '.rjust(0, chr(42)), (n12 // 5)).splitlines()], [i + v for i, v in enumerate([a * b for a, b in zip(([ord(c) for c in '\U0001f600 ok'] + ([5, 255] * 3)), sorted({s17 % 4 for s17 in [1000, 100]}))], 0)]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
