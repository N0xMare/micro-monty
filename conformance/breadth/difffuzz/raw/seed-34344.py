# difffuzz divergence, seed 34344
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: 'x\ny****'
#   mm:      EXC MemoryError 
# signature: CPython: EXC ValueError invalid literal for int() with base #: 'x\ny****' | mm: EXC MemoryError 
import math
try:
    def make7(start):
        total = start
        def step(by=1):
            return total
    s = make7([((2 + -17) if True else +-3), (2 ** 64 ** (round(-2.25) % 7)), int('x\ny'.ljust(7, chr(42)), 16)]['123'.zfill(4).center(7, chr(45)).count('{}-{!r}-{:>4}'.format(-17, 'mississippi', 5).capitalize())])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
