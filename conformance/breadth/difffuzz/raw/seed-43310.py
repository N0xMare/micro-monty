# difffuzz divergence, seed 43310
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u20cb'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⃋'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cb' | mm: EXC ValueError invalid literal for int() with base #: '⃋'
import math
try:
    print(int(chr(8395)), end='')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
