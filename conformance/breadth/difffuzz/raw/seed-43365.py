# difffuzz divergence, seed 43365
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0bbc'
#   mm:      EXC ValueError invalid literal for int() with base 10: '஼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#bbc' | mm: EXC ValueError invalid literal for int() with base #: '஼'
import math
try:
    def make19(start):
        total = start
        def step(by=1):
            return total
    s = make19(int(chr(3004)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
