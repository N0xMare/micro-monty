# difffuzz divergence, seed 37014
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0db2'
#   mm:      EXC ValueError invalid literal for int() with base 16: '඲'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#db#' | mm: EXC ValueError invalid literal for int() with base #: '඲'
import math
try:
    match int(chr(3506), 16):
        case [x, y, *rest]:
            print("many", x, y, rest)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
