# difffuzz divergence, seed 2395
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u2e78'
#   mm:      EXC ValueError invalid literal for int() with base 36: '⹸'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '⹸'
import math
try:
    class C16:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C16({self.v!r}, {self.w})'
    objs = [C16(abs(divmod(round((0.0 ** 2)), len([ord(c) for c in 'mississippi']))[0]).bit_length(), 0), C16(int(chr(11896), 36), -3), C16(pow(f2(), 35, 7), 0), C16.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
