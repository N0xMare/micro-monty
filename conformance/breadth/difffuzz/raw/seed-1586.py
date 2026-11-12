# difffuzz divergence, seed 1586
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u05cc'
#   mm:      EXC ValueError invalid literal for int() with base 0: '׌'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cc' | mm: EXC ValueError invalid literal for int() with base #: '׌'
import math
try:
    class C7:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C7({self.v!r}, {self.w})'
    objs = [C7(1, 4), C7(abs(int(chr(1484), 0)).bit_length(), 4), C7(-abs(100).bit_length(), -2), C7.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
