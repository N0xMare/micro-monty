# difffuzz divergence, seed 29591
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0bfc'
#   mm:      EXC ValueError invalid literal for int() with base 10: '௼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#bfc' | mm: EXC ValueError invalid literal for int() with base #: '௼'
import math
try:
    class C24:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C24(int('a,b,,c'.split(',')[2].join(list({'k0': 5, 'k1': 7, 'k2': -1, 'k3': -17})).join(chr(3068).split(' '))), 4), C24(round((((-1 / 100) ** 0.5) / -0.0)), 4), C24(pow((((v1 >> (v1 % 9)) if (1e-05 <= v1) else sum([42, -17, -17])) + ~f8()), 0, 7), -2), C24.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
