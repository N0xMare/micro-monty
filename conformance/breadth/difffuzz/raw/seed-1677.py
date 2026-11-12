# difffuzz divergence, seed 1677
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u171d'
#   mm:      EXC ValueError invalid literal for int() with base 0: '᜝'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d' | mm: EXC ValueError invalid literal for int() with base #: '᜝'
import math
try:
    v6 = chr(5917)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C19:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C19({self.v!r}, {self.w})'
    objs = [C19(2 ** 64, 3), C19(int(v6.lstrip(chr(10965)), 0), 4), C19(divmod(divmod(abs(pow(i2, 16, -5)).bit_length(), divmod((-17 - n14), {'k0': 7}['k1'])[1])[0], (sum(list(range(5, 11, 1))) << (v6.index('123'.swapcase()) % 70)))[1], 2), C19.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
