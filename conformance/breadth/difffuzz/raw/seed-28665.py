# difffuzz divergence, seed 28665
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1bf8'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᯸'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#bf#' | mm: EXC ValueError invalid literal for int() with base #: '᯸'
import math
try:
    def make22(start):
        total = start
        def step(by=1):
            return total
    s = make22(int(chr(7160)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
