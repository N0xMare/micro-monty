# difffuzz divergence, seed 23425
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err3 '\u2d2f' 6 wrapped
#   mm:      Err3 '⴯' 6 wrapped
# signature: CPython: Err# '\u#d#f' # wrapped | mm: Err# '⴯' # wrapped
import math
try:
    class Err3(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err3(chr(11567), 6)
        except Err3 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
