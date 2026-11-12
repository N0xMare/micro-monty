# difffuzz divergence, seed 20346
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u20cc'
#   mm:      EXC ValueError invalid literal for int() with base 16: '⃌'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cc' | mm: EXC ValueError invalid literal for int() with base #: '⃌'
import math
try:
    print(math.ceil(abs(int(chr(8396), 16)) % 30))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
