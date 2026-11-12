# difffuzz divergence, seed 43456
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u187a'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᡺'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a' | mm: EXC ValueError invalid literal for int() with base #: '᡺'
import math
try:
    def make12(start):
        total = start
        def step(by=1):
            return total
    s = make12(((1000 & int(chr(6266))) ** ({k13: len(k13) for k13 in list({'k0': -1, 'k1': 255, 'k2': -1})}['zz'] % 7)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
