# difffuzz divergence, seed 16227
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u244e'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⑎'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e' | mm: EXC ValueError invalid literal for int() with base #: '⑎'
import math
try:
    class C16:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C16(int(chr(9294)), 4), C16(divmod({str(q): q for q in list(filter(lambda q: q > -1, [-1]))}['zz'], ((-(2 ** 70) >> ((10 ** (3 % 7)) % 9)) >> (len(list({'k0': 1000, 'k1': 3, 'k2': 255, 'k3': 3})) % 9)))[0], 2), C16(int(round(float(repr({})), -2)), 3), C16.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
