# difffuzz divergence, seed 40218
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err7 "7-'mississippi'-1000\u0e6b7-'mississippi'-1000{7-'mississippi'-1000\u0e6b7-'mississippi'-1000}7-'mississippi'-1000\u0e6b7-'mississippi'-1000" 4 wrapped
#   mm:      Err7 "7-'mississippi'-1000๫7-'mississippi'-1000{7-'mississippi'-1000๫7-'mississippi'-1000}7-'mississippi'-1000๫7-'mississippi'-1000" 4 wrapped
# signature: CPython: Err# "#-'mississippi'-#\u#e#b#-'mississippi'-#{#-'mississippi'-#\u#e#b#-'mississippi'-#}#-'mississip | mm: Err# "#-'mississippi'-#๫#-'mississippi'-#{#-'mississippi'-#๫#-'mississippi'-#}#-'mississippi'-#๫#-'m
import math
try:
    class Err7(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err7(repr({}).swapcase().replace('\U0001f600 ok'.join([]).rjust(0, chr(42)), chr(3691).replace('a'.partition(',')[1], '{}-{!r}-{:>4}'.format(7, 'mississippi', 1000))).zfill(3), 4)
        except Err7 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
