# difffuzz divergence, seed 14543
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u0eff'
#   mm:      EXC ValueError invalid literal for int() with base 36: '໿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#eff' | mm: EXC ValueError invalid literal for int() with base #: '໿'
import math
try:
    class C4:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C4({self.v!r}, {self.w})'
    objs = [C4((max(list(map(lambda q: q * 2 + 1, [len(w) for w in ['mississippi']]))) + (1 << 100) - 1), -1), C4(pow(int(chr(3839), 36), 10, 1000), 0), C4(math.ceil(1e-05), 4), C4.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
