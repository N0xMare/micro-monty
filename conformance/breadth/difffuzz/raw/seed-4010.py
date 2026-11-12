# difffuzz divergence, seed 4010
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err2 '\u10ce' 1 wrapped
#   mm:      Err2 '჎' 1 wrapped
# signature: CPython: Err# '\u#ce' # wrapped | mm: Err# '჎' # wrapped
import math
try:
    n1 = 6
    while n1 > 0:
        n1 -= 1
        data = list('')
        class Err2(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err2(chr(4302), 1)
            except Err2 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
