# difffuzz divergence, seed 12866
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err15 '\u1975{}{}{}' 6 wrapped
#   mm:      Err15 '᥵{}{}{}' 6 wrapped
# signature: CPython: Err# '\u#{}{}{}' # wrapped | mm: Err# '᥵{}{}{}' # wrapped
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v8 = str(isinstance([a * b for a, b in zip(lst0, [1])][:], int))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class Err15(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err15((chr(6517) + (repr({}) * (~v8.index(v8) % 4))), 6)
        except Err15 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
