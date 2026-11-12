# difffuzz divergence, seed 34077
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err13 '\u196e' 3 wrapped
#   mm:      Err13 '᥮' 3 wrapped
# signature: CPython: Err# '\u#e' # wrapped | mm: Err# '᥮' # wrapped
import math
try:
    class Err13(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err13(chr(6510), 3)
        except Err13 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
