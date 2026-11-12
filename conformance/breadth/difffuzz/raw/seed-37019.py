# difffuzz divergence, seed 37019
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err6 '\u0a7b' 9 wrapped
#   mm:      Err6 '੻' 9 wrapped
# signature: CPython: Err# '\u#a#b' # wrapped | mm: Err# '੻' # wrapped
import math
try:
    class Err6(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err6(chr(2683), 9)
        except Err6 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
