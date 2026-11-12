# difffuzz divergence, seed 34555
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '0000\u171b'
#   mm:      EXC ValueError invalid literal for int() with base 10: '0000᜛'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#b' | mm: EXC ValueError invalid literal for int() with base #: '#᜛'
import math
try:
    class C7:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C7(int(chr(5915).zfill(5)), 1), C7({f'{-1:08} and {1e-05}{None:*^9} and '[6:]: ((-17 - 5) >> ((-3 ** (255 % 7)) % 9))}['k2'], 1), C7(int(chr(123).split(' ')[0], 0), -2), C7.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
