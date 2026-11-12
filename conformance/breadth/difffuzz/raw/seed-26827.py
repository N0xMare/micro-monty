# difffuzz divergence, seed 26827
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2fdb'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⿛'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fdb' | mm: EXC ValueError invalid literal for int() with base #: '⿛'
import math
try:
    v25 = abs((int(chr(12251)) / int(round(-0.0, 2))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
