# difffuzz divergence, seed 39293
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u16fe'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᛾'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fe' | mm: EXC ValueError invalid literal for int() with base #: '᛾'
import math
try:
    lst0 = [3, 1, 2]
    lst0.insert(5, int(chr(5886)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
