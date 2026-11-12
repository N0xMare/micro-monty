# difffuzz divergence, seed 3858
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: "'\\u0d99'"
#   mm:      EXC ValueError invalid literal for int() with base 0: "'඙'"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "'\\u#d#'" | mm: EXC ValueError invalid literal for int() with base #: "'඙'"
import math
try:
    def make22(start):
        total = start
        def step(by=1):
            return total
    s = make22(int(repr(chr(3481)), 0))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
