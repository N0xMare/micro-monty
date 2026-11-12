# difffuzz divergence, seed 8131
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u208f'
#   mm:      EXC ValueError invalid literal for int() with base 10: '₏'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f' | mm: EXC ValueError invalid literal for int() with base #: '₏'
import math
try:
    if ((1 << 100) - 1 < ([3, 42] + [-17])[:][int(chr(8335))] <= int(((1000 << (n11 % 70)) / (n11 if True else n11)))):
        while n14 > 0:
            def make15(start):
                total = start
                def step(by=1):
                    return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
