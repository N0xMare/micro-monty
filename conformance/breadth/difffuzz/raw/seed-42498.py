# difffuzz divergence, seed 42498
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0a4a'
#   mm:      EXC ValueError invalid literal for int() with base 16: '੊'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#a' | mm: EXC ValueError invalid literal for int() with base #: '੊'
import math
try:
    class C1:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C1({self.v!r}, {self.w})'
    objs = [C1(sum(list(map(lambda q: q * -2 + 1, list(map(lambda q: q * -2 + 1, list(map(lambda q: q * 2 + 1, [-17]))))))), 4), C1(int(chr(2634), 16), 2), C1(int(repr(list('hello')).zfill(1)), -3), C1.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
