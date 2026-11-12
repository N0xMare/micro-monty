# difffuzz divergence, seed 1802
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0a49'
#   mm:      EXC ValueError invalid literal for int() with base 10: '੉'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#' | mm: EXC ValueError invalid literal for int() with base #: '੉'
import math
try:
    def make13(start):
        total = start
        def step(by=1):
            return total
    s = make13((len({str(q): q for q in sorted([2])}) << (int(chr(2633).expandtabs()) % 70)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
