# difffuzz divergence, seed 18401
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u245d'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⑝'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d' | mm: EXC ValueError invalid literal for int() with base #: '⑝'
import math
try:
    class C7:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C7({self.v!r}, {self.w})'
    objs = [C7(divmod(pow(len(({'k0': 2, 'k1': -3, 'k2': 1} | {'k0': 10, 'k1': 0})), 23, -5), 1)[1], -2), C7(((int(chr(9309)) ** ({k8: len(k8) for k8 in ['Hello World']}['k0'] % 7)) >> (int('123'.rjust(8, chr(45)).title()) % 9)), 2), C7(i1, 2), C7.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
