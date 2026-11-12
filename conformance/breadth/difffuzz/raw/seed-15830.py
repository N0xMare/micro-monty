# difffuzz divergence, seed 15830
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err1 "-3-'\\u0e5e'--10000000000000000000000000" 0 wrapped
#   mm:      Err1 "-3-'๞'--10000000000000000000000000" 0 wrapped
# signature: CPython: Err# "-#-'\\u#e#e'--#" # wrapped | mm: Err# "-#-'๞'--#" # wrapped
import math
try:
    class Err1(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err1('{}-{!r}-{:>4}'.format((-3 | sum(([42, 100, 1000, 255] + [2, 7, 3]))), chr(3678), -(10 ** 25)), 0)
        except Err1 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
