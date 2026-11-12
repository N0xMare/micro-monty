# difffuzz divergence, seed 33185
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '**\u0ef7'
#   mm:      EXC ValueError invalid literal for int() with base 36: '**໷'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '**\u#ef#' | mm: EXC ValueError invalid literal for int() with base #: '**໷'
import math
try:
    class C9:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C9(int((chr(3831).rjust(3, chr(42)) * (abs(round(1e300)).bit_length() % 4)), 36), 1), C9((divmod(abs((n1 << (n1 % 70))).bit_length(), [a * b for a, b in zip([100, 1000, 1000, 0], lst0)][lst0[-1]])[1] + ((~0 // divmod(n1, n1)[0]) ** ([i + v for i, v in enumerate(lst0, 1)][100] % 7))), -2), C9((int(('  pad  ' * (n1 % 4)).swapcase(), 10) >> ((-(10 ** 25) if isinstance('\u00df'.swapcase(), bool) else ((5 << (n1 % 70)) - f4(n1))) % 9)), 2), C9.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
