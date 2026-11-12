# difffuzz divergence, seed 34037
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err7 25-'\u0d52'--590295810358705651712 4 wrapped
#   mm:      Err7 25-'൒'--590295810358705651712 4 wrapped
# signature: CPython: Err# #-'\u#d#'--# # wrapped | mm: Err# #-'൒'--# # wrapped
import math
try:
    v6 = (-1 << (len(dict(sorted({}.items(), key=lambda kv: kv[1]))) % 70))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class Err7(Exception):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err7('{}-{!r}-{:>4}'.format(math.ceil(abs(max([ord(c) for c in 'mississippi'])) % 30), chr(3410), (('a,b,,c'.count('mississippi') | v6) << ((+-1 ** ((v6 >> (5 % 9)) % 7)) % 70))), 4)
        except Err7 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
