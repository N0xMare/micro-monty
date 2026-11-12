# difffuzz divergence, seed 5154
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err24 '\u07b6ড়₲' 1 wrapped
#   mm:      Err24 '޶ড়₲' 1 wrapped
# signature: CPython: Err# '\u#b#ড়₲' # wrapped | mm: Err# '޶ড়₲' # wrapped
import math
try:
    class Err24(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err24((chr(1974).rstrip() + (chr(2524) + chr(8370))), 1)
        except Err24 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
