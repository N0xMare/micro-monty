# difffuzz divergence, seed 20998
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err18 '\u0b59' 6 wrapped
#   mm:      Err18 '୙' 6 wrapped
# signature: CPython: Err# '\u#b#' # wrapped | mm: Err# '୙' # wrapped
import math
try:
    class Err18(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err18(chr(2905), 6)
        except Err18 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
