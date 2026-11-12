# difffuzz divergence, seed 36473
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u177c'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᝼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c' | mm: EXC ValueError invalid literal for int() with base #: '᝼'
import math
try:
    def f8(p0):
        v9 = sorted(['\U0001f600 ok'], key=lambda w: (-len(w), w))
        def rec10(n):
            if n <= 1:
                return n
        return (divmod(int(chr(6012)), ((p0 if True else p0) & ~p0))[1] << (-1 % 70))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C11:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C11(f8(f8(f8((5 ** (5 % 7))))), -1), C11(int(-round((-17 / 10), 4)), 3), C11(abs(int('%%%d' % ((-17 << (100 % 70)),), 36)).bit_length(), -3), C11.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
