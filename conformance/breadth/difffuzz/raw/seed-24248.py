# difffuzz divergence, seed 24248
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err20 '\u09ff' 1 wrapped
#   mm:      Err20 '৿' 1 wrapped
# signature: CPython: Err# '\u#ff' # wrapped | mm: Err# '৿' # wrapped
import math
try:
    n19 = 1
    while n19 > 0:
        class Err20(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err20(chr(2559), 1)
            except Err20 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
        print('lst0', lst0)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
