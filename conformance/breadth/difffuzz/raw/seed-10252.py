# difffuzz divergence, seed 10252
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0ee2'
#   mm:      EXC ValueError invalid literal for int() with base 10: '໢'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ee#' | mm: EXC ValueError invalid literal for int() with base #: '໢'
import math
try:
    class C1:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C1({self.v!r}, {self.w})'
    objs = [C1(int(-0.0), 0), C1((3 >> (abs((sum([]) if bool({'k0': 42, 'k1': 1, 'k2': 1000}) else int(2.5))).bit_length() % 9)), -3), C1(int(chr(3810), 10), -3), C1.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
