# difffuzz divergence, seed 32252
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1ae1'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᫡'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ae#' | mm: EXC ValueError invalid literal for int() with base #: '᫡'
import math
try:
    class C10:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C10(abs(int(chr(6881))).bit_length(), -4), C10(+(1 << 100) - 1, 3), C10(max((sorted({42, -1, 0, 1000, -17}) + [(int(2.5) + i11) for i11 in (lst0 + lst0)])), -2), C10.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
