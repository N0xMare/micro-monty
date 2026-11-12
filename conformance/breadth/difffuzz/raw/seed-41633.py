# difffuzz divergence, seed 41633
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u20fe'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⃾'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fe' | mm: EXC ValueError invalid literal for int() with base #: '⃾'
import math
try:
    v25 = (int(chr(8446)) in {10, 1, 100, 1000, 7})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
