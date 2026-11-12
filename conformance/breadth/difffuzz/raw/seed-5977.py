# difffuzz divergence, seed 5977
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1c4b'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᱋'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c#b' | mm: EXC ValueError invalid literal for int() with base #: '᱋'
import math
try:
    class C2:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C2({self.v!r}, {self.w})'
    objs = [C2(len(({} | {'k0': 2, 'k1': 3, 'k2': 5})), 2), C2((int(chr(7243)) // 255), 1), C2(round(math.atan((math.tau / math.tau))), -4), C2.make(1)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
