# difffuzz divergence, seed 32709
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u135c-----------'
#   mm:      EXC ValueError invalid literal for int() with base 10: '፜-----------'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c-----------' | mm: EXC ValueError invalid literal for int() with base #: '፜-----------'
import math
try:
    def make31(start):
        total = start
        def step(by=1):
            return total
    s = make31(-int(chr(4956).ljust(12, chr(45))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
