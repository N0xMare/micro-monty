# difffuzz divergence, seed 42834
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u2ef9'
#   mm:      EXC ValueError invalid literal for int() with base 36: '⻹'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ef#' | mm: EXC ValueError invalid literal for int() with base #: '⻹'
import math
try:
    class C1:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C1(abs(int(chr(12025), 36)).bit_length(), 3), C1(round(3.0), 3), C1((5 >> (math.floor(abs(max([0, 255, 5])) % 30) % 9)), 1), C1.make(3)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
