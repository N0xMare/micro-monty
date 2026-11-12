# difffuzz divergence, seed 3510
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err26 '\u0ff9' 7 wrapped
#   mm:      Err26 '࿹' 7 wrapped
# signature: CPython: Err# '\u#ff#' # wrapped | mm: Err# '࿹' # wrapped
import math
try:
    class Err26(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err26(chr(4089).title().rstrip(), 7)
        except Err26 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
