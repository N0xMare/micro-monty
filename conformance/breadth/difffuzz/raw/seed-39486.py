# difffuzz divergence, seed 39486
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '----\u1a7d-----'
#   mm:      EXC ValueError invalid literal for int() with base 10: '----᩽-----'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '----\u#a#d-----' | mm: EXC ValueError invalid literal for int() with base #: '----᩽-----'
import math
try:
    def make5(start):
        total = start
        def step(by=1):
            return total
    s = make5(math.ceil(abs(int(chr(6781).center(10, chr(45)))) % 30))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
