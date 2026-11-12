# difffuzz divergence, seed 30980
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u0ee0'
#   mm:      EXC ValueError invalid literal for int() with base 2: '໠'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ee#' | mm: EXC ValueError invalid literal for int() with base #: '໠'
import math
try:
    if (len(tuple([7, 10, 100])[:1:1]) < int(chr(3808).expandtabs(), 2) <= [0][(pow(10, 0, 7) if (-1 in [1, 100, 100, 5]) else -3)]):
        def gen5(n):
                    yield k * 2
        def make6(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
