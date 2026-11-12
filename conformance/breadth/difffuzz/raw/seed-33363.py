# difffuzz divergence, seed 33363
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0be5'
#   mm:      EXC ValueError invalid literal for int() with base 10: '௥'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#be#' | mm: EXC ValueError invalid literal for int() with base #: '௥'
import math
try:
    n1 = int(chr(3045))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
