# difffuzz divergence, seed 25379
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err8 '\u19dd' 6 wrapped
#   mm:      Err8 '᧝' 6 wrapped
# signature: CPython: Err# '\u#dd' # wrapped | mm: Err# '᧝' # wrapped
import math
try:
        data = [str(q) for q in list(range(3, 11, 2))]
        for i5 in range(0, 8):
            n6 = 2
            while n6 > 0:
                n6 -= 1
                v7 = {255, 5}
        class Err8(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err8(chr(6621), 6)
            except Err8 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
