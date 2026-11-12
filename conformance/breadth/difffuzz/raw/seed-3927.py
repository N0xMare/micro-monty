# difffuzz divergence, seed 3927
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u05f7'
#   mm:      EXC ValueError invalid literal for int() with base 0: '׷'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f#' | mm: EXC ValueError invalid literal for int() with base #: '׷'
import math
try:
    class C2:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C2((((round(1e300) << ((10 * 1) % 70)) if 'a'.swapcase().startswith(''.swapcase()) else abs(abs(0).bit_length()).bit_length()) ** (int(chr(1527)[(-1 ** (5 % 7))], 0) % 7)), 0), C2(round(abs(('  pad  '.find('AbC') / divmod(255, 7)[0]))), 1), C2(pow(int(float('a')), 13, 7), 4), C2.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
