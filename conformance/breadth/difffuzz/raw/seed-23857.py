# difffuzz divergence, seed 23857
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u19cc'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᧌'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cc' | mm: EXC ValueError invalid literal for int() with base #: '᧌'
import math
try:
    n17 = 2
    while n17 > 0:
        n17 -= 1
        def rec18(n):
            if n <= 1:
                return n
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(float(int(chr(6604).removesuffix('  pad  '[n17]))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
