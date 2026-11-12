# difffuzz divergence, seed 11837
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u20ff'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⃿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ff' | mm: EXC ValueError invalid literal for int() with base #: '⃿'
import math
try:
    def make3(start):
        total = start
        def step(by=1):
            return total
    s = make3(int((chr(8447) * ((-17 >> (-1 % 9)) % 4))[0:1:2]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
