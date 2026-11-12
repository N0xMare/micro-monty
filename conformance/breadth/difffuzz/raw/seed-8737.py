# difffuzz divergence, seed 8737
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0fdb'
#   mm:      EXC ValueError invalid literal for int() with base 10: '࿛'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fdb' | mm: EXC ValueError invalid literal for int() with base #: '࿛'
import math
try:
    for i20 in [a * b for a, b in zip([int(math.sin(3.0)), int(chr(4059))], list({}.values()))]:
        def make21(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
