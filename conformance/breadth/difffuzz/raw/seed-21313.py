# difffuzz divergence, seed 21313
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2e71'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⹱'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '⹱'
import math
try:
    def risky():
        try:
            print(list(range(5, 3, 1))[int(chr(11889))])
        finally:
            print("finally", len({-1, 100, 1}))
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
