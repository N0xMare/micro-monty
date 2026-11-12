# difffuzz divergence, seed 19962
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0bd9'
#   mm:      EXC ValueError invalid literal for int() with base 10: '௙'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#bd#' | mm: EXC ValueError invalid literal for int() with base #: '௙'
import math
try:
    def make22(start):
        total = start
        def step(by=1):
            return total
    s = make22(int('x\ny'.expandtabs().lstrip().join(chr(3033).splitlines())))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
