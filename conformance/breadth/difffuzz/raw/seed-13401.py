# difffuzz divergence, seed 13401
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u1979'
#   mm:      EXC ValueError invalid literal for int() with base 0: '᥹'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '᥹'
import math
try:
    for i6 in range(-2, 6):
        def gen7(n):
            for k in range(n):
                    yield k * -3
        it = gen7(9)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f12(p0, *rest, kw=0):
        return '\U0001f600 ok'
    print('f12(int((chr(6521) * (i6 % 4)), 0))', f12(int((chr(6521) * (i6 % 4)), 0)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
