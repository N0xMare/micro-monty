# difffuzz divergence, seed 43251
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u209d'
#   mm:      EXC ValueError invalid literal for int() with base 10: '₝'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d' | mm: EXC ValueError invalid literal for int() with base #: '₝'
import math
try:
    for i4 in range(1, 8):
        if i4 == 0:
            break
        def make5(start):
            total = start
            def step(by=1):
                return total
        s = make5(int(chr(8349)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
