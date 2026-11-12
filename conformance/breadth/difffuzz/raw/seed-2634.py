# difffuzz divergence, seed 2634
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: (-1, -1180591620717411303169)3.5EXC ValueError invalid literal for int() with base 10: '540954828182817.2*\u245a'
#   mm:      (-1, -1180591620717411303169)3.5EXC ValueError invalid literal for int() with base 10: '540954828182817.2*⑚'
# signature: CPython: (-#, -#)#.#EXC ValueError invalid literal for int() with base #: '#.#*\u#a' | mm: (-#, -#)#.#EXC ValueError invalid literal for int() with base #: '#.#*⑚'
import math
try:
    v8 = chr(9306)[:6:2].center(2, chr(42)).swapcase()
    v8 += repr(math.e)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(divmod(255, -(2 ** 70)), end='3.5')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C11:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C11(int(f'{v8:*^9} '.split(None)[-1][::-1]), 0), C11(f5(), 1), C11(-sum([i + v for i, v in enumerate([-17][4:-5:1], 2)]), 1), C11.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
