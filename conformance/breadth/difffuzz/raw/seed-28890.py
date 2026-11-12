# difffuzz divergence, seed 28890
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0e76'
#   mm:      EXC ValueError invalid literal for int() with base 10: '๶'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '๶'
import math
try:
    class C1:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C1(int('{}-{!r}-{:>4}'.format(divmod(-(10 ** 25), 255)[1], 'tab\there', int(chr(3702))), 10), 1), C1(abs(abs('tab\there'.expandtabs().count('tab\there'[7:])).bit_length()).bit_length(), 2), C1(42, -4), C1.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
