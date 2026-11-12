# difffuzz divergence, seed 38835
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err4 '\u0cf7' 0 wrapped
#   mm:      Err4 '೷' 0 wrapped
# signature: CPython: Err# '\u#cf#' # wrapped | mm: Err# '೷' # wrapped
import math
try:
    class Err4(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err4(chr(3319), 0)
        except Err4 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
