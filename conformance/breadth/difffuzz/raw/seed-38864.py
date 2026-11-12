# difffuzz divergence, seed 38864
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1c4b'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᱋'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c#b' | mm: EXC ValueError invalid literal for int() with base #: '᱋'
import math
try:
    n24 = 5
    while n24 > 0:
        match '-42':
            case [] | ():
                print("empty")
        def make25(start):
            total = start
            def step(by=1):
                return total
        s = make25(int(chr(7243)))
        class C26:
            def __init__(self, v, w=0):
                self.w = w
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
