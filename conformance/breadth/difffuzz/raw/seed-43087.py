# difffuzz divergence, seed 43087
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2d9d'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⶝'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#d' | mm: EXC ValueError invalid literal for int() with base #: '⶝'
import math
try:
    n2 = 5
    while n2 > 0:
        n2 -= 1
        for i3 in range(-2, 5):
            if i3 == 2:
                break
            def make4(start):
                total = start
                def step(by=1):
                    return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C8:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C8({self.v!r}, {self.w})'
    objs = [C8((n2 ^ [ord(c) for c in '\U0001f600 ok'.lstrip()][((0 ** (i3 % 7)) if (5 in [7, 1000, 1, 1000, 3]) else (n2 // 1000))]), -4), C8(int(chr(11677)), 1), C8(~abs(len([])).bit_length(), 3), C8.make(5)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
