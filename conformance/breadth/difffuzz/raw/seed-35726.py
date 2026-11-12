# difffuzz divergence, seed 35726
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u2d2f'
#   mm:      EXC ValueError invalid literal for int() with base 2: '⴯'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#f' | mm: EXC ValueError invalid literal for int() with base #: '⴯'
import math
try:
    class C2:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C2({self.v!r}, {self.w})'
    objs = [C2(pow(-3, 7, 7), -2), C2(int(chr(11567), 2), 3), C2((int('', 10) - (list(reversed([100, 10, 2, 255])) + list(range(-2, -2, 1)))[-17]), 3), C2.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
