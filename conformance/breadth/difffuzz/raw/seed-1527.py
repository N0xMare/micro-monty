# difffuzz divergence, seed 1527
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u2d7d'
#   mm:      EXC ValueError invalid literal for int() with base 0: '⵽'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#d' | mm: EXC ValueError invalid literal for int() with base #: '⵽'
import math
try:
    def make16(start):
        total = start
        def step(by=1):
            return total
    s = make16(int(chr(11645).expandtabs(), 0))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
