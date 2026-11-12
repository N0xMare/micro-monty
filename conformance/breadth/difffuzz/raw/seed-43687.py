# difffuzz divergence, seed 43687
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2fe0'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⿠'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fe#' | mm: EXC ValueError invalid literal for int() with base #: '⿠'
import math
try:
    class C13:
        def __init__(self, v, w=0):
            self.w = w
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C28(C13):
        def __init__(self, v, w=0):
            self.w = w
    objs = [C28(+int(chr(12256)), 2), C28(pow(i22, 13, 1000), -1), C28(round(3.0), 0), C28.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
