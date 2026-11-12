# difffuzz divergence, seed 12236
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err25 ['\u1774'] 7 wrapped
#   mm:      Err25 0000['᝴'] 7 wrapped
# signature: CPython: Err# ['\u#'] # wrapped | mm: Err# #['᝴'] # wrapped
import math
try:
    v1 = {42}
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f24(p0):
        class Err25(LookupError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err25(repr(chr(6004).split()).zfill(9), 7)
            except Err25 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
    print('f24(sum(list({str(q): q for q in sorted(v1)}.values())))', f24(sum(list({str(q): q for q in sorted(v1)}.values()))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
