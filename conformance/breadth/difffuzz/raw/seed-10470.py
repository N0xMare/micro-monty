# difffuzz divergence, seed 10470
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1771'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᝱'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '᝱'
import math
try:
    class C9:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C9(+int(chr(6001)), -4), C9(math.factorial(-(divmod(0, 10)[1] / round(1e300))), -4), C9(max(list(map(lambda q: q * 2 + 1, [i + v for i, v in enumerate([5, -17, 0, 10, -3], 1)][:]))), -1), C9.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
