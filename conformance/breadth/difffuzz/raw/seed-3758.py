# difffuzz divergence, seed 3758
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u05fd'
#   mm:      EXC ValueError invalid literal for int() with base 0: '׽'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fd' | mm: EXC ValueError invalid literal for int() with base #: '׽'
import math
try:
    def f7(p0):
        return 3 ** 50
    print('f7(f7(int(chr(1533).swapcase(), 0)))', f7(f7(int(chr(1533).swapcase(), 0))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
