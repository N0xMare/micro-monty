# difffuzz divergence, seed 15546
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 10: '\u0d45' ("invalid literal for int() with base 10: '\\u0d45'",)
#   mm:      caught ValueError invalid literal for int() with base 10: '൅' ("invalid literal for int() with base 10: '൅'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '\u#d#' ("invalid literal for int() with ba | mm: caught ValueError invalid literal for int() with base #: '൅' ("invalid literal for int() with base #
import math
try:
    n4 = 6
    while n4 > 0:
        n4 -= 1
        try:
            print([1, 2].remove(int(chr(3397))))
        except ValueError as e:
            print("caught", type(e).__name__, e, e.args)
        def make7(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
