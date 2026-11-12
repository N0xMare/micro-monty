# difffuzz divergence, seed 10861
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '**********\u17fb'
#   mm:      EXC ValueError invalid literal for int() with base 10: '**********៻'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '**********\u#fb' | mm: EXC ValueError invalid literal for int() with base #: '**********៻'
import math
try:
    n1 = 2
    while n1 > 0:
        class C2:
            def __init__(self, v, w=0):
                self.w = w
            def __repr__(self):
                return f'C2({self.v!r}, {self.w})'
        objs = [C2(divmod(n1, abs(pow('  pad  '.find('-42'), 28, -5)).bit_length())[1], 2), C2(pow(int(chr(6139).rjust(11, chr(42)), 10), 31, 1000), 4), C2(divmod([ord(c) for c in 'hello'[7]][pow(int(3.0), 24, 1000)], max([len(w) for w in ['AbC', '3.5']]))[0], 3), C2.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
