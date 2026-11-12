# difffuzz divergence, seed 21416
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0d84'
#   mm:      EXC ValueError invalid literal for int() with base 10: '඄'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#' | mm: EXC ValueError invalid literal for int() with base #: '඄'
import math
try:
    class C1:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C1({self.v!r}, {self.w})'
    objs = [C1(len(repr((-1, 0, )).rjust(9, chr(45)).split(',')), 2), C1(round(1e300), -4), C1(abs(int(chr(3460))).bit_length(), 1), C1.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
