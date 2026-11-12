# difffuzz divergence, seed 9891
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u09b1'
#   mm:      EXC ValueError invalid literal for int() with base 0: '঱'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '঱'
import math
try:
    for i21 in range(1):
        if i21 == 2:
            break
        def gen22(n):
            for k in range(n):
                    yield k * -1
        it = gen22(9)
        for i23 in range(2, 6):
            match None:
                case int() | float():
                    print("number")
            def make24(start):
                total = start
                def step(by=1):
                    return total
            s = make24(int(chr(2481), 0))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
