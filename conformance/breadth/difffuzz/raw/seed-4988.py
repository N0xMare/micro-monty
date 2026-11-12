# difffuzz divergence, seed 4988
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err27 '\u2d69' 7 wrapped
#   mm:      Err27 '⵩' 7 wrapped
# signature: CPython: Err# '\u#d#' # wrapped | mm: Err# '⵩' # wrapped
import math
try:
    class Err27(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err27(chr(11625), 7)
        except Err27 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
