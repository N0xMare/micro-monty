# difffuzz divergence, seed 37161
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err19 '**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ\u2431**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ' 7 wrapped
#   mm:      Err19 '**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ␱**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ' 7 wrapped
# signature: CPython: Err# '**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ\u#**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ' # wrapped | mm: Err# '**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ␱**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ-**ԐԐ' # wrapped
import math
try:
    v5 = chr(1296)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class Err19(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err19(chr(9265).center(12, chr(45)).replace(v5[8::1].replace((str((5, 1000, 5, 1, )) + v5), v5[-5:3:-2].strip('hello')), (v5 + v5).rjust(4, chr(42)).rjust(1, chr(42))), 7)
        except Err19 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
