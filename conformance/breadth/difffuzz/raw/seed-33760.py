# difffuzz divergence, seed 33760
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err1 '\u2e73' 4 wrapped
#   mm:      Err1 '⹳' 4 wrapped
# signature: CPython: Err# '\u#e#' # wrapped | mm: Err# '⹳' # wrapped
import math
try:
    class Err1(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err1(chr(11891), 4)
        except Err1 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
