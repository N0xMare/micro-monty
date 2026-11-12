# difffuzz divergence, seed 14294
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err16 '\u18fc' 3 wrapped
#   mm:      Err16 '᣼' 3 wrapped
# signature: CPython: Err# '\u#fc' # wrapped | mm: Err# '᣼' # wrapped
import math
try:
    class Err16(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err16(chr(6396), 3)
        except Err16 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
