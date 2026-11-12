# difffuzz divergence, seed 2624
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err31 '\u10cf' 4 wrapped
#   mm:      Err31 '჏' 4 wrapped
# signature: CPython: Err# '\u#cf' # wrapped | mm: Err# '჏' # wrapped
import math
try:
    class Err31(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err31(chr(4303), 4)
        except Err31 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
