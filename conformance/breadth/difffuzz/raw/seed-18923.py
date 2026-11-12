# difffuzz divergence, seed 18923
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err1 1-'\u0a64'-  11 8 wrapped
#   mm:      Err1 1-'੤'-  11 8 wrapped
# signature: CPython: Err# #-'\u#a#'-  # # wrapped | mm: Err# #-'੤'-  # # wrapped
import math
try:
    class Err1(Exception):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err1('{}-{!r}-{:>4}'.format(1, chr(2660), len('Hello World')), 8)
        except Err1 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
