# difffuzz divergence, seed 28207
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err2 '\u18ac' 6 wrapped
#   mm:      Err2 '᢬' 6 wrapped
# signature: CPython: Err# '\u#ac' # wrapped | mm: Err# '᢬' # wrapped
import math
try:
    for i1 in range(4):
        if i1 == 3:
            continue
        class Err2(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err2(chr(6316), 6)
            except Err2 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
