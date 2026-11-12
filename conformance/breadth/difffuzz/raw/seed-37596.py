# difffuzz divergence, seed 37596
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1f5a'
#   mm:      EXC ValueError invalid literal for int() with base 10: '὚'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f#a' | mm: EXC ValueError invalid literal for int() with base #: '὚'
import math
try:
    print([(int(chr(8026)) + i8) for i8 in range(6) if i8 % 2])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
