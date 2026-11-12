# difffuzz divergence, seed 40455
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0adf'
#   mm:      EXC ValueError invalid literal for int() with base 10: '૟'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#adf' | mm: EXC ValueError invalid literal for int() with base #: '૟'
import math
try:
    def make4(start):
        total = start
        def step(by=1):
            return total
    s = make4(math.isqrt(abs(int(chr(2783))) % 30))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
