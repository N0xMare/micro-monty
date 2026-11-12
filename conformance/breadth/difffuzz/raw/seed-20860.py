# difffuzz divergence, seed 20860
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u1aec'
#   mm:      EXC ValueError invalid literal for int() with base 16: '᫬'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#aec' | mm: EXC ValueError invalid literal for int() with base #: '᫬'
import math
try:
    class C1:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C1(divmod(sum(sorted({0})), (int(chr(6892), 16) >> (2 ** 64 % 9)))[0], 1), C1(len(set([42, 3, 255, -17])), 1), C1((10 ** 30 + 7 >> ((int('  pad  ') << (int(float(7)) % 70)) % 9)), -3), C1.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
