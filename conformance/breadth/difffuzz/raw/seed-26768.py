# difffuzz divergence, seed 26768
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u2d69'
#   mm:      EXC ValueError invalid literal for int() with base 2: '⵩'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#' | mm: EXC ValueError invalid literal for int() with base #: '⵩'
import math
try:
    class C8:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C8(int(chr(11625).removesuffix(chr(699)).removesuffix(str({}).rstrip()), 2), 0), C8(2 ** 64, -3), C8(divmod(abs(int(1e10)).bit_length(), (pow(math.trunc(abs(10) % 30), 1, 7) >> (int('%d' % ('3.5',)) % 9)))[1], -3), C8.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
