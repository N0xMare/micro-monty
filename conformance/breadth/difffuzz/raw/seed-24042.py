# difffuzz divergence, seed 24042
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2e69'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⹩'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '⹩'
import math
try:
    class C11:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C11({self.v!r}, {self.w})'
    objs = [C11(pow((sum([-1]) * sum(list({'k0': 1000, 'k1': 10, 'k2': 0}.values()))), 1, 7), 4), C11(pow(int(chr(11881)[3:-3:-2]), 14, 7), -1), C11(int(''.join(['Hello World']).replace('caf\u00e9'.replace('123', '\u00df'), f'{n9:e} and {7.125:.3e}').join(list(({'k0': 3, 'k1': 0, 'k2': 100} | {'k0': 0}))), 2), 1), C11.make(1)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
