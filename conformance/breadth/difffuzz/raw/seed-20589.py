# difffuzz divergence, seed 20589
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err26 '\u1cbb' 5 wrapped
#   mm:      Err26 '᲻' 5 wrapped
# signature: CPython: Err# '\u#cbb' # wrapped | mm: Err# '᲻' # wrapped
import math
try:
    class Err26(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err26(chr(7355), 5)
        except Err26 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
