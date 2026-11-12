# difffuzz divergence, seed 6327
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0a0e'
#   mm:      EXC ValueError invalid literal for int() with base 10: '਎'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#e' | mm: EXC ValueError invalid literal for int() with base #: '਎'
import math
try:
    n18 = int(chr(2574).expandtabs())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
