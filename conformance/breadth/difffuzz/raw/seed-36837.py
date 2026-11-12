# difffuzz divergence, seed 36837
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u082e'
#   mm:      EXC ValueError invalid literal for int() with base 16: '࠮'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e' | mm: EXC ValueError invalid literal for int() with base #: '࠮'
import math
try:
    class C23:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C23(int(chr(2094), 16), -4), C23((pow(int('mississippi'.partition(' ')[1], 0), 15, 2 ** 61 - 1) >> (sorted([len(w) for w in []], reverse=True)[[len(w) for w in ['a']][{}['k1']]] % 9)), -4), C23(int(((str([-1, 100]) + '%%%d' % (2,)) * (('\u00df'.index('a,b,,c') >> (abs(v21).bit_length() % 9)) % 4))), -2), C23.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
