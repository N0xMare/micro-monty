# difffuzz divergence, seed 25836
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u218f'
#   mm:      EXC ValueError invalid literal for int() with base 10: '↏'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f' | mm: EXC ValueError invalid literal for int() with base #: '↏'
import math
try:
    def make8(start):
        total = start
        def step(by=1):
            return total
    s = make8(int(chr(8591), 10))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
