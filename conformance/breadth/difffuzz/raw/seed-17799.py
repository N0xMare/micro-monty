# difffuzz divergence, seed 17799
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0e61'
#   mm:      EXC ValueError invalid literal for int() with base 10: '๡'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '๡'
import math
try:
    v5 = int(chr(3681))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
