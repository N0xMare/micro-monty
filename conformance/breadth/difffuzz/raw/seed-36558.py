# difffuzz divergence, seed 36558
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err22 '\u09ff' 3 wrapped
#   mm:      Err22 '৿' 3 wrapped
# signature: CPython: Err# '\u#ff' # wrapped | mm: Err# '৿' # wrapped
import math
try:
    class Err22(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err22(chr(2559).split(',')[-1].partition(',')[0], 3)
        except Err22 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
