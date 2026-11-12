# difffuzz divergence, seed 17488
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0b7b'
#   mm:      EXC ValueError invalid literal for int() with base 10: '୻'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#b' | mm: EXC ValueError invalid literal for int() with base #: '୻'
import math
try:
    if (w := math.isqrt(abs(int(chr(2939))) % 30)) > 6:
        print("walrus", w)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
