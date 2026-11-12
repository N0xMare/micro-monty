# difffuzz divergence, seed 3707
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u20f1'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⃱'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f#' | mm: EXC ValueError invalid literal for int() with base #: '⃱'
import math
try:
    lst0 = [3, 1, 2]
    lst0.insert(0, int(chr(8433)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
