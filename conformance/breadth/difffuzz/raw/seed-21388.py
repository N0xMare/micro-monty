# difffuzz divergence, seed 21388
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err20 '\u19cf' 7 wrapped
#   mm:      Err20 '᧏' 7 wrapped
# signature: CPython: Err# '\u#cf' # wrapped | mm: Err# '᧏' # wrapped
import math
try:
    class Err20(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err20(chr(6607), 7)
        except Err20 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
