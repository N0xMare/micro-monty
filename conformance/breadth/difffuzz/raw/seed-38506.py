# difffuzz divergence, seed 38506
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err33 0-'-\u20c7'-717897987691852588770249 2 wrapped
#   mm:      Err33 0-'-⃇'-717897987691852588770249 2 wrapped
# signature: CPython: Err# #-'-\u#c#'-# # wrapped | mm: Err# #-'-⃇'-# # wrapped
import math
try:
    n30 = 1
    while n30 > 0:
        n30 -= 1
        class Ctx31:
            def __init__(self, name):
                self.name = name
            def __enter__(self):
                return self
            def __exit__(self, kind, value, tb):
                return False
        with Ctx31("a") as x, Ctx31("b"):
            pass
        v32 = (42, -1, 100, -17, )
        class Err33(LookupError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err33('{}-{!r}-{:>4}'.format(pow(abs('3.5'.rfind('3.5')).bit_length(), 34, 2 ** 61 - 1), ('-42'[n30][-5:] + chr(8391).replace('tab\there'[:7], '\U0001f600 ok'[n30])), 3 ** 50), 2)
            except Err33 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
