# difffuzz divergence, seed 40796
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0de5'
#   mm:      EXC ValueError invalid literal for int() with base 10: '෥'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#de#' | mm: EXC ValueError invalid literal for int() with base #: '෥'
import math
try:
    for i4 in range(0, 8):
        class Err5(LookupError):
            def __init__(self, msg, code=0):
                self.code = code
        try:
            try:
                raise Err5('{}-{!r}-{:>4}'.format(int(chr(3557).upper()), '\u00df', -(10 ** 25)), 9)
            except Err5 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
        def f8(p0, p1):
            class Ctx9:
                def __init__(self, name):
                    self.name = name
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
