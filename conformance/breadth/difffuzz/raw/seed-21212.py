# difffuzz divergence, seed 21212
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err2 '\u0be3' 5 wrapped
#   mm:      Err2 '௣' 5 wrapped
# signature: CPython: Err# '\u#be#' # wrapped | mm: Err# '௣' # wrapped
import math
try:
    class Err2(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err2(chr(3043), 5)
        except Err2 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
