# difffuzz divergence, seed 26789
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err5 '\u12c7' 9 wrapped
#   mm:      Err5 '዇' 9 wrapped
# signature: CPython: Err# '\u#c#' # wrapped | mm: Err# '዇' # wrapped
import math
try:
    class Err5(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err5(chr(4807), 9)
        except Err5 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
