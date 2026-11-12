# difffuzz divergence, seed 20619
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u1af6'
#   mm:      EXC ValueError could not convert string to float: '᫶'
# signature: CPython: EXC ValueError could not convert string to float: '\u#af#' | mm: EXC ValueError could not convert string to float: '᫶'
import math
try:
    class C19:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C19({self.v!r}, {self.w})'
    objs = [C19(10 ** 30 + 7, -4), C19(int(float(chr(6902))), 2), C19((int(float(repr((100, )))) if (max(sorted({100, 1000, 5, -1})) in ({42, -1, 0} ^ frozenset(v3) | {3})) else pow(pow((42 ** (1000 % 7)), 3, 2 ** 61 - 1), 7, 2 ** 61 - 1)), -3), C19.make(1)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
