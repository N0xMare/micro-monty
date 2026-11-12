# difffuzz divergence, seed 15834
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2d78'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⵸'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#' | mm: EXC ValueError invalid literal for int() with base #: '⵸'
import math
try:
    n3 = 5
    while n3 > 0:
        lst0 = [3, 1, 2]
        print((int(chr(11640)) * pow((divmod(1, -17)[0] ** ((2 << (n3 % 70)) % 7)), 31, 7)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
