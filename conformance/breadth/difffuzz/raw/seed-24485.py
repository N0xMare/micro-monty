# difffuzz divergence, seed 24485
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u13ff'
#   mm:      EXC ValueError invalid literal for int() with base 36: '᏿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ff' | mm: EXC ValueError invalid literal for int() with base #: '᏿'
import math
try:
    def make22(start):
        total = start
        def step(by=1):
            return total
    s = make22(pow(int(chr(5119).expandtabs(), 36), 12, 2 ** 61 - 1))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
