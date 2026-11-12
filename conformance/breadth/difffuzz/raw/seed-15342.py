# difffuzz divergence, seed 15342
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u2073'
#   mm:      EXC ValueError invalid literal for int() with base 2: '⁳'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '⁳'
import math
try:
    def f6(p0, p1):
        try:
            print(repr(v1).replace((('123'.join([]) + ''.split(',')[2]) + str({str(q): q for q in [5, 1, 3]})), str(tuple([len(w) for w in ['a']]))) + pow(-(2 ** 70), 16, 1000))
        except ValueError as e:
            print("caught", type(e).__name__, e, e.args)
    print('f6([3, -17, -17, 100][math.floor(abs(int(chr(8307), 2)) % 30)], -(2 ** 70))', f6([3, -17, -17, 100][math.floor(abs(int(chr(8307), 2)) % 30)], -(2 ** 70)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
