# difffuzz divergence, seed 21674
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u2d26'
#   mm:      EXC ValueError invalid literal for int() with base 0: '⴦'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#' | mm: EXC ValueError invalid literal for int() with base #: '⴦'
import math
try:
    class C32:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C32(math.trunc(abs((int(chr(11558), 0) >> (round(float(1000)) % 9))) % 30), -4), C32((len({v13.zfill(0): (i24 & n2), v13: abs(n2).bit_length()}) ** (2 ** 64 % 7)), 3), C32((round(+1.5) << (int(str(lst0).partition('s')[0], 16) % 70)), 1), C32.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
