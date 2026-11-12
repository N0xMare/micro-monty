# difffuzz divergence, seed 29149
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: "'\\u19cb'"
#   mm:      EXC ValueError invalid literal for int() with base 10: "'᧋'"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "'\\u#cb'" | mm: EXC ValueError invalid literal for int() with base #: "'᧋'"
import math
try:
        def rec10(n):
            if n <= 1:
                return n
        class C11:
            def __init__(self, v, w=0):
                self.w = w
        objs = [C11(abs(int(repr(chr(6603)), 10)).bit_length(), -4), C11(({}['k0'] >> (1000 % 9)), -4), C11((pow((v6.rfind('a') + sum([])), 13, 1000) << (~len({1000, 10, 100, 7}) % 70)), -3), C11.make(2)]
        match ((2, ) + (100, -3, )):
            case [x, y, *rest]:
                print("many", x, y, rest)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
