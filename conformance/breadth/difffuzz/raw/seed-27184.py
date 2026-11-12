# difffuzz divergence, seed 27184
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err1 "10-'\\u0c64'-   0" 9 wrapped
#   mm:      Err1 "10-'౤'-   0" 9 wrapped
# signature: CPython: Err# "#-'\\u#c#'-   #" # wrapped | mm: Err# "#-'౤'-   #" # wrapped
import math
try:
    class Err1(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err1('{}-{!r}-{:>4}'.format({'k0': 10, 'k1': 255, 'k2': 5, 'k3': 1000}['k0'], chr(3172), divmod(1, -1)[1]).lstrip('a'.join([]).ljust(6, chr(42))).rjust(11, chr(42)), 9)
        except Err1 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
