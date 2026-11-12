# difffuzz divergence, seed 44192
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err9 ['\u2dcf'] 8 wrapped
#   mm:      Err9 ['⷏'] 8 wrapped
# signature: CPython: Err# ['\u#dcf'] # wrapped | mm: Err# ['⷏'] # wrapped
import math
try:
    class Err9(ValueError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err9(repr(chr(11727).splitlines()), 8)
        except Err9 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
