# difffuzz divergence, seed 19549
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0b58'
#   mm:      EXC ValueError invalid literal for int() with base 10: '୘'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '୘'
import math
try:
        for i20 in range(8):
            print(int(chr(2904).split(None)[0]))
        def gen21(n):
                if k % 3 == 0:
                    yield k * 0
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
