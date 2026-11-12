# difffuzz divergence, seed 13783
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u0ef5'
#   mm:      EXC ValueError invalid literal for int() with base 36: '໵'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ef#' | mm: EXC ValueError invalid literal for int() with base #: '໵'
import math
try:
    def make5(start):
        total = start
        def step(by=1):
            return total
    s = make5(int(chr(3829), 36))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
