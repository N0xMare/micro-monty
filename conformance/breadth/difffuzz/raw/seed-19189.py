# difffuzz divergence, seed 19189
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2439'
#   mm:      EXC ValueError invalid literal for int() with base 10: '␹'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '␹'
import math
try:
    class C18:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C18({self.v!r}, {self.w})'
    objs = [C18(-(10 ** 25), -2), C18(int(chr(9273).rstrip()), 0), C18(+f2(list(map(lambda q: q * 0 + 1, [(5 + i19) for i19 in range(-3, 6) if i19 % 2]))), 4), C18.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
