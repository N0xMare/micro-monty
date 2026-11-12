# difffuzz divergence, seed 6233
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err4 '\u19cb' 5 wrapped
#   mm:      Err4 '᧋' 5 wrapped
# signature: CPython: Err# '\u#cb' # wrapped | mm: Err# '᧋' # wrapped
import math
try:
    class Err4(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err4(chr(6603), 5)
        except Err4 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
