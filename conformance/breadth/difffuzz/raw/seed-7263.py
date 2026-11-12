# difffuzz divergence, seed 7263
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '00\u2eff'
#   mm:      EXC ValueError invalid literal for int() with base 10: '00⻿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#eff' | mm: EXC ValueError invalid literal for int() with base #: '#⻿'
import math
try:
    def f22(p0, p1, p2):
        if True:
            def f23(p0):
                return chr(9007)
                if n <= 1:
                    return n
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def make30(start):
        total = start
        def step(by=1):
            return total
    s = make30(f22(-(10 ** 25), int(chr(12031).zfill(3)), int('hello'.split()[-1].title(), 16)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
