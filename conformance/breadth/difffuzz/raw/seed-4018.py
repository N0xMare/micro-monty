# difffuzz divergence, seed 4018
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err2 18446744073709551616-'\u0ee3'-10000000000 8 wrapped
#   mm:      Err2 18446744073709551616-'໣'-10000000000 8 wrapped
# signature: CPython: Err# #-'\u#ee#'-# # wrapped | mm: Err# #-'໣'-# # wrapped
import math
try:
    class Err2(LookupError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err2('{}-{!r}-{:>4}'.format(2 ** 64, chr(3811), +int(1e10)).zfill(5), 8)
        except Err2 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
