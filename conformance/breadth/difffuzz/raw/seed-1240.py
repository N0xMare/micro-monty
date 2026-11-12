# difffuzz divergence, seed 1240
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u20f3'
#   mm:      EXC ValueError could not convert string to float: '⃳'
# signature: CPython: EXC ValueError could not convert string to float: '\u#f#' | mm: EXC ValueError could not convert string to float: '⃳'
import math
try:
    class C11:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C11({self.v!r}, {self.w})'
    objs = [C11(pow(min(list(filter(lambda q: q > 3, sorted([-17, 1000], key=str)))), 18, 2 ** 61 - 1), 4), C11(math.isqrt(abs(round(float(chr(8435)))) % 30), 4), C11((divmod(len([str(q) for q in [42, 7, 1, 42]]), {}['k0'])[1] if bool(-(0.1 ** -1)) else math.trunc(math.inf)), -4), C11.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
