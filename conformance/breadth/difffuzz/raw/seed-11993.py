# difffuzz divergence, seed 11993
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0e5e'
#   mm:      EXC ValueError invalid literal for int() with base 10: '๞'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#e' | mm: EXC ValueError invalid literal for int() with base #: '๞'
import math
try:
    class C15:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C15({self.v!r}, {self.w})'
    objs = [C15(round(0.0), 0), C15(pow(divmod(int(chr(3678), 10), math.trunc(2.5))[0], 12, 2 ** 61 - 1), 2), C15(int(float('AbC'.split()[1].partition('s')[1])), 2), C15.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
