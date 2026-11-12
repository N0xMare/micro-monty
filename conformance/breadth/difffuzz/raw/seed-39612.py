# difffuzz divergence, seed 39612
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2fdb'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⿛'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fdb' | mm: EXC ValueError invalid literal for int() with base #: '⿛'
import math
try:
    lst0[2] = int(chr(12251))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
