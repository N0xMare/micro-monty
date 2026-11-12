# difffuzz divergence, seed 25685
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u058b'
#   mm:      EXC ValueError invalid literal for int() with base 2: '֋'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b' | mm: EXC ValueError invalid literal for int() with base #: '֋'
import math
try:
    class C15:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C15({self.v!r}, {self.w})'
    objs = [C15(int((round(-1e300, 2) % 3.0)), 2), C15(int(chr(1419), 2), 3), C15(10 ** 30 + 7, 0), C15.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
