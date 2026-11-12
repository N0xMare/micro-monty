# difffuzz divergence, seed 41877
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u05fb'
#   mm:      EXC ValueError invalid literal for int() with base 0: '׻'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fb' | mm: EXC ValueError invalid literal for int() with base #: '׻'
import math
try:
    v7 = (divmod(sum([]), int(chr(1531), 0)) + (divmod(int(0.0), (42 if True else 100)) + tuple([][:-2:2])))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
