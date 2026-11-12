# difffuzz divergence, seed 24253
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u1a8c'
#   mm:      EXC ValueError invalid literal for int() with base 16: '᪌'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#c' | mm: EXC ValueError invalid literal for int() with base #: '᪌'
import math
try:
    class Err1(ValueError):
        def __init__(self, msg, code=0):
            self.code = code
    try:
        try:
            raise Err1('{}-{!r}-{:>4}'.format(int(chr(6796), 16), '\u00df'.swapcase().partition(',')[1][::1], math.floor(abs(int(1e10)) % 30)), 9)
        except Err1 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
