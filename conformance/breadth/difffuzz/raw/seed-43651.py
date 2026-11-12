# difffuzz divergence, seed 43651
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err15 '\u2e62' 5 wrapped
#   mm:      Err15 '⹢' 5 wrapped
# signature: CPython: Err# '\u#e#' # wrapped | mm: Err# '⹢' # wrapped
import math
try:
    class Err15(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err15(chr(11874).removeprefix(repr('caf\u00e9')), 5)
        except Err15 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
