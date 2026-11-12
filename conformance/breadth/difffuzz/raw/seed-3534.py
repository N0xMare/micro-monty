# difffuzz divergence, seed 3534
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2fe2'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⿢'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fe#' | mm: EXC ValueError invalid literal for int() with base #: '⿢'
import math
try:
    match divmod(int(chr(12258)), abs(((0 >> (v6 % 9)) ** (divmod(-17, v6)[0] % 7))).bit_length()):
        case int(n) if n > 10:
            print("big", n)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
