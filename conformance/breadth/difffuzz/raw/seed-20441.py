# difffuzz divergence, seed 20441
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0e6c'
#   mm:      EXC ValueError invalid literal for int() with base 10: '๬'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#c' | mm: EXC ValueError invalid literal for int() with base #: '๬'
import math
try:
    class C29:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C29(divmod(divmod(int(chr(3692), 10), int('{}-{!r}-{:>4}'.format(i17, '123', 1), 16))[1], ((--1 ** (abs(-1).bit_length() % 7)) ** ((len([255]) ** (math.factorial(v13) % 7)) % 7)))[1], -3), C29(pow((int(v13) >> ((lst0[-17] & (-3 ** (-3 % 7))) % 9)), 9, 1000), -2), C29(divmod(int(abs(7.125)), int((divmod(100, i15)[1] / [100, 7][i15])))[1], 4), C29.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
