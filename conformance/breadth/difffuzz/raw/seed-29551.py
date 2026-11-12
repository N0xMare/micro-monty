# difffuzz divergence, seed 29551
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err16 '\u0c3b' 0 wrapped
#   mm:      Err16 '఻' 0 wrapped
# signature: CPython: Err# '\u#c#b' # wrapped | mm: Err# '఻' # wrapped
import math
try:
        class Err16(LookupError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err16(repr(chr(3131)), 0)
            except Err16 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
