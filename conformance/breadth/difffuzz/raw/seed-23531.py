# difffuzz divergence, seed 23531
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err19 😀 ok0-'\u0fde'- 355 1 wrapped
#   mm:      Err19 😀 ok0-'࿞'- 355 1 wrapped
# signature: CPython: Err# 😀 ok#-'\u#fde'- # # wrapped | mm: Err# 😀 ok#-'࿞'- # # wrapped
import math
try:
    class Err19(LookupError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err19(('\U0001f600 ok' + '{}-{!r}-{:>4}'.format(int(-0.0), chr(4062), sum([255, 100]))).rjust(6, chr(45)), 1)
        except Err19 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
