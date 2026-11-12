# difffuzz divergence, seed 35454
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err23 '\u2d2c' 6 wrapped
#   mm:      Err23 '⴬' 6 wrapped
# signature: CPython: Err# '\u#d#c' # wrapped | mm: Err# '⴬' # wrapped
import math
try:
    class Err23(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err23(chr(11564), 6)
        except Err23 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
