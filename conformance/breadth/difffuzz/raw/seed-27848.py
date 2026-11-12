# difffuzz divergence, seed 27848
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0af2'
#   mm:      EXC ValueError invalid literal for int() with base 10: '૲'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#af#' | mm: EXC ValueError invalid literal for int() with base #: '૲'
import math
try:
    v10 = chr(4520)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    lst0 = [3, 1, 2]
    lst0.insert(-1, int(chr(2802).replace(v10.removesuffix(v10).zfill(5), v10.removeprefix(v10))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
