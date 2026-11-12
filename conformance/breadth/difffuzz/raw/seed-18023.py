# difffuzz divergence, seed 18023
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u0cf4'
#   mm:      EXC ValueError invalid literal for int() with base 36: '೴'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cf#' | mm: EXC ValueError invalid literal for int() with base #: '೴'
import math
try:
    def make54(start):
        total = start
        def step(by=1):
            return total
    s = make54(((int(chr(3316), 36) >> (int(v31.partition(',')[1]) % 9)) ** (f33(v31.zfill(9).join([str(q) for q in lst0]), +(v52 << (i42 % 70))) % 7)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
