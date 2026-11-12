# difffuzz divergence, seed 35434
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err20 0-'\u0eeb'-   0AbC 5 wrapped
#   mm:      Err20 0-'໫'-   0AbC 5 wrapped
# signature: CPython: Err# #-'\u#eeb'-   #AbC # wrapped | mm: Err# #-'໫'-   #AbC # wrapped
import math
try:
    class Err20(LookupError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err20('{}-{!r}-{:>4}'.format(round(-0.0), chr(3819), round(0.1)).lstrip().join(['', 'AbC']), 5)
        except Err20 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
