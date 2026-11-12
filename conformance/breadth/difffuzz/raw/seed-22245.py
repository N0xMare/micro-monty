# difffuzz divergence, seed 22245
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1755'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᝕'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '᝕'
import math
try:
    n6 = 4
    while n6 > 0:
        while n7 > 0:
            def gen8(n):
                    if k % 4 == 0:
                        yield k * 3
        def make9(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C10:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C10({self.v!r}, {self.w})'
    objs = [C10(n6, 4), C10(int(chr(5973).replace('a,b,,c'.ljust(6, chr(42)).rjust(4, chr(45)), '\U0001f600 ok'.strip('caf\u00e9').swapcase())), -1), C10(math.factorial((round(round(0.1, 3), -2) % abs(math.pi))), 3), C10.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
