# difffuzz divergence, seed 32357
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2efd'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⻽'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#efd' | mm: EXC ValueError invalid literal for int() with base #: '⻽'
import math
try:
    def make5(start):
        total = start
        def step(by=1):
            return total
    s = make5((int(chr(12029)) << (([255, 3, 3, -17, 2][:-3] + [7, 1000, 5, 5])[int(repr({}), 0)] % 70)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
