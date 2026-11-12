# difffuzz divergence, seed 10463
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err9 '\u2dbf' 1 wrapped
#   mm:      Err9 '⶿' 1 wrapped
# signature: CPython: Err# '\u#dbf' # wrapped | mm: Err# '⶿' # wrapped
import math
try:
    class Err9(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err9(chr(11711), 1)
        except Err9 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
