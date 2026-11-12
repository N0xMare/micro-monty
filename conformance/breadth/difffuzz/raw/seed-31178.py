# difffuzz divergence, seed 31178
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err5 '\u218f' 2 wrapped
#   mm:      Err5 '↏' 2 wrapped
# signature: CPython: Err# '\u#f' # wrapped | mm: Err# '↏' # wrapped
import math
try:
    class Err5(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err5(chr(8591), 2)
        except Err5 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
