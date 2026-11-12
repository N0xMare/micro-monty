# difffuzz divergence, seed 5082
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0a55'
#   mm:      EXC ValueError invalid literal for int() with base 10: '੕'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#' | mm: EXC ValueError invalid literal for int() with base #: '੕'
import math
try:
    class C21:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C21({self.v!r}, {self.w})'
    objs = [C21(math.floor(-0.0), -3), C21(pow(int(chr(2645)), 31, 1000), -3), C21((({str(q): q for q in [255, 255, 42, 7, 0]} | {k22: len(k22) for k22 in ['123', 'caf\u00e9']})['k0'] >> ([len(w) for w in list('mississippi')][(int('3.5', 0) >> (f9('mississippi', n7, '\U0001f600 ok') % 9))] % 9)), -1), C21.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
