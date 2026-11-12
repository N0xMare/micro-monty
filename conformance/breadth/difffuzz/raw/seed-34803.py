# difffuzz divergence, seed 34803
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u0e78'
#   mm:      EXC ValueError invalid literal for int() with base 0: '๸'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '๸'
import math
try:
    class C14:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C14({self.v!r}, {self.w})'
    objs = [C14(len('3.5'.zfill(5)), 3), C14(sum([1000, 42, 7, -1]), -3), C14(int(chr(3704), 0), 0), C14.make(1)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
