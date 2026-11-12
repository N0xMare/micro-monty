# difffuzz divergence, seed 24748
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u20cb'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⃋'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cb' | mm: EXC ValueError invalid literal for int() with base #: '⃋'
import math
try:
    def make3(start):
        total = start
        def step(by=1):
            return total
    s = make3(math.trunc(abs((int(chr(8395)) ^ 100)) % 30))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
