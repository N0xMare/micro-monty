# difffuzz divergence, seed 12544
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u2fe1'
#   mm:      EXC ValueError invalid literal for int() with base 0: '⿡'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fe#' | mm: EXC ValueError invalid literal for int() with base #: '⿡'
import math
try:
    def make14(start):
        total = start
        def step(by=1):
            return total
    s = make14((7 << (int(chr(12257), 0) % 70)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
