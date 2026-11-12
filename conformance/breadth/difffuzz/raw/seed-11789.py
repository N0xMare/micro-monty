# difffuzz divergence, seed 11789
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0b84'
#   mm:      EXC ValueError invalid literal for int() with base 10: '஄'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '஄'
import math
try:
    lst0 = [3, 1, 2]
    print(lst0.index(+int(chr(2948))), lst0.count(math.isqrt(abs(abs(pow(divmod(100, -17)[1], 10, 1000)).bit_length()) % 30)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
