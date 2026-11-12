# difffuzz divergence, seed 36373
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err11 '\u0a04' 4 wrapped
#   mm:      Err11 '਄' 4 wrapped
# signature: CPython: Err# '\u#a#' # wrapped | mm: Err# '਄' # wrapped
import math
try:
    class Err11(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err11(chr(2564), 4)
        except Err11 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
