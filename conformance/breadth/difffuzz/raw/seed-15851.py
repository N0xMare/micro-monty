# difffuzz divergence, seed 15851
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2fef'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⿯'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fef' | mm: EXC ValueError invalid literal for int() with base #: '⿯'
import math
try:
    class C8:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C8(int(chr(12271)), -2), C8(math.trunc(math.sin(2.5)), -4), C8(divmod(1000, int(math.log10((10 / 3))))[1], 2), C8.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
