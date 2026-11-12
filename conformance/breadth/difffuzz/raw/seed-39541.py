# difffuzz divergence, seed 39541
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err23 3-"'\\u242b'"-1203 9 wrapped
#   mm:      Err23 3-"'␫'"-1203 9 wrapped
# signature: CPython: Err# #-"'\\u#b'"-# # wrapped | mm: Err# #-"'␫'"-# # wrapped
import math
try:
    class Err23(LookupError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err23('{}-{!r}-{:>4}'.format(int(math.pi), repr(chr(9259)), sum([1000, 3, 100, 100])), 9)
        except Err23 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
