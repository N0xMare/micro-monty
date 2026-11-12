# difffuzz divergence, seed 28633
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0a11'
#   mm:      EXC ValueError could not convert string to float: '਑'
# signature: CPython: EXC ValueError could not convert string to float: '\u#a#' | mm: EXC ValueError could not convert string to float: '਑'
import math
try:
    class C5:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C5((round(float(chr(2577))) >> (math.factorial(abs(-(2 ** 70)) % 30) % 9)), -4), C5(~(((2 & 255) ** (-(2 ** 70) % 7)) if True else '{}-{!r}-{:>4}'.format(10, v1, -3).index(v1.replace(v1, v1))), -1), C5(int(v1.removesuffix((f'{[7, 255, -1, 1, 3]!r}' + chr(425))), 0), 2), C5.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
