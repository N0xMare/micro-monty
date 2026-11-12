# difffuzz divergence, seed 42071
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err4 '\u2e72' 6 wrapped
#   mm:      Err4 '⹲' 6 wrapped
# signature: CPython: Err# '\u#e#' # wrapped | mm: Err# '⹲' # wrapped
import math
try:
        n3 = 4
        while n3 > 0:
            n3 -= 1
            class Err4(KeyError):
                def __init__(self, msg, code=0):
                    super().__init__(msg)
                    self.code = code
            try:
                try:
                    raise Err4(chr(11890), 6)
                except Err4 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
