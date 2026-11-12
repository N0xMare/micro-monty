# difffuzz divergence, seed 11220
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err3 '-17-a-5-3-a-55-a-51-a-5Hello World-a-5a-a-5mississippi\u1aae' 3 wrapped
#   mm:      Err3 '-17-a-5-3-a-55-a-51-a-5Hello World-a-5a-a-5mississippi᪮' 3 wrapped
# signature: CPython: Err# '-#-a-#-#-a-#-a-#-a-#Hello World-a-#a-a-#mississippi\u#aae' # wrapped | mm: Err# '-#-a-#-#-a-#-a-#-a-#Hello World-a-#a-a-#mississippi᪮' # wrapped
import math
try:
    v1 = '{}-{!r}-{:>4}'.format(max([255, 5, 10]), 'a'.expandtabs(), abs(1000).bit_length())[7::-2].join(list(({str(q): q for q in [-17, -3, 5, 1]} | dict(zip(['Hello World', 'a', 'mississippi', ''], [0, 1, 0])))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class Err3(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err3((v1 + chr(6830)), 3)
        except Err3 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
