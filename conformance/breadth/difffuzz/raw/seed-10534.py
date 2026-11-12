# difffuzz divergence, seed 10534
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u242a'
#   mm:      EXC ValueError invalid literal for int() with base 2: '␪'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a' | mm: EXC ValueError invalid literal for int() with base #: '␪'
import math
try:
    v17 = (int(chr(9258), 2) < round(((10 / i12) ** 3)) <= sum(lst0[:]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
