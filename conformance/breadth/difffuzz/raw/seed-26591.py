# difffuzz divergence, seed 26591
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1ad9'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᫙'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ad#' | mm: EXC ValueError invalid literal for int() with base #: '᫙'
import math
try:
    n1 = 1
    while n1 > 0:
        class C2:
            def __init__(self, v, w=0):
                self.w = w
            def __repr__(self):
                return f'C2({self.v!r}, {self.w})'
        objs = [C2(len(chr(8064).splitlines()), 0), C2(int(chr(6873).lstrip()), -2), C2(((-{'k0': 100}['k0'] << ((+1000 ** ('\U0001f600 ok'.index('a') % 7)) % 70)) >> (int('\U0001f600 ok'[n1][-3:0], 10) % 9)), -2), C2.make(4)]
        try:
            print(math.sqrt(((int(abs(1.5)) / max([len(w) for w in ['caf\u00e9', '-42']])) ** -1)))
        except ArithmeticError as e:
            print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
