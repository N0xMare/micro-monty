# difffuzz divergence, seed 44028
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u18ab'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᢫'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ab' | mm: EXC ValueError invalid literal for int() with base #: '᢫'
import math
try:
    def make9(start):
        total = start
        def step(by=1):
            return total
    s = make9((int(chr(6315)) ** ((list({}.values())[(100 << (255 % 70))] >> ((n4 ** (-(10 ** 25) % 7)) % 9)) % 7)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
