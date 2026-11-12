# difffuzz divergence, seed 39318
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u18f8'
#   mm:      EXC ValueError invalid literal for int() with base 2: '᣸'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f#' | mm: EXC ValueError invalid literal for int() with base #: '᣸'
import math
try:
    def make2(start):
        total = start
        def step(by=1):
            return total
    s = make2(int(chr(6392).center(1, chr(45))[-1::-1], 2))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
