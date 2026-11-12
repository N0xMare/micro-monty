# difffuzz divergence, seed 9865
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err1 '\u0ef9' 6 wrapped
#   mm:      Err1 '໹' 6 wrapped
# signature: CPython: Err# '\u#ef#' # wrapped | mm: Err# '໹' # wrapped
import math
try:
    class Err1(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err1(chr(3833).lstrip('x\ny'.join(['3.5', '123'])).rstrip()[-3:2], 6)
        except Err1 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
