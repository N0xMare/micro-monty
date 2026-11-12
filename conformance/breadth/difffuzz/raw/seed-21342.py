# difffuzz divergence, seed 21342
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err15 "{'k0': -3, 'k1': 0, 'k2': -1}\u0e7c" 2 wrapped
#   mm:      Err15 "{'k0': -3, 'k1': 0, 'k2': -1}๼" 2 wrapped
# signature: CPython: Err# "{'k#': -#, 'k#': #, 'k#': -#}\u#e#c" # wrapped | mm: Err# "{'k#': -#, 'k#': #, 'k#': -#}๼" # wrapped
import math
try:
    class Err15(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err15((repr({'k0': -3, 'k1': 0, 'k2': -1}) + chr(3708).title().capitalize()), 2)
        except Err15 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
