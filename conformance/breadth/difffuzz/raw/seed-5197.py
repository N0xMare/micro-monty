# difffuzz divergence, seed 5197
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err10 '\u2efc' 6 wrapped
#   mm:      Err10 '⻼' 6 wrapped
# signature: CPython: Err# '\u#efc' # wrapped | mm: Err# '⻼' # wrapped
import math
try:
    class Err10(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err10(chr(12028), 6)
        except Err10 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
