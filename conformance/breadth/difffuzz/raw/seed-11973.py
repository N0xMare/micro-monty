# difffuzz divergence, seed 11973
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '000\u0f48'
#   mm:      EXC ValueError invalid literal for int() with base 10: '000཈'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#f#' | mm: EXC ValueError invalid literal for int() with base #: '#཈'
import math
try:
    def make10(start):
        total = start
        def step(by=1):
            return total
    s = make10(int(chr(3912).split()[-1].zfill(4)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
