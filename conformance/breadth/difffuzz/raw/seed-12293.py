# difffuzz divergence, seed 12293
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u2e66'
#   mm:      EXC ValueError invalid literal for int() with base 0: '⹦'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '⹦'
import math
try:
    class C2:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C2({self.v!r}, {self.w})'
    objs = [C2(math.trunc(abs(5) % 30), 2), C2(round(-round(math.e, -1)), 3), C2(int(chr(11878), 0), -3), C2.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
