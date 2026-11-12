# difffuzz divergence, seed 18011
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err19 'Hello\u0dcd' 9 wrapped
#   mm:      Err19 'Hello෍' 9 wrapped
# signature: CPython: Err# 'Hello\u#dcd' # wrapped | mm: Err# 'Hello෍' # wrapped
import math
try:
    v5 = f'{1.5:e}'.join(sorted('hello'.split(','), key=lambda w: w[::-1])).title()
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class Err19(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err19((v5.expandtabs().lstrip().zfill(3) + chr(3533)), 9)
        except Err19 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
