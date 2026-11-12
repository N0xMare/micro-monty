# difffuzz divergence, seed 26285
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u2d75'
#   mm:      EXC ValueError invalid literal for int() with base 16: '⵵'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#' | mm: EXC ValueError invalid literal for int() with base #: '⵵'
import math
try:
    for i5 in range(7):
        if i5 == 1:
            break
        def make6(start):
            total = start
            def step(by=1):
                return total
        s = make6(round((sum([1000, -1]) / int(chr(11637), 16))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
