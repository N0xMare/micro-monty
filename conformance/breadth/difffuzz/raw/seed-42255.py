# difffuzz divergence, seed 42255
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err5 '((\'\', 3), ("-17-\'😀 ok\'- 100", -1), (\'\\u0bdf\', 1))' 3 wrapped
#   mm:      Err5 '((\'\', 3), ("-17-\'😀 ok\'- 100", -1), (\'௟\', 1))' 3 wrapped
# signature: CPython: Err# '((\'\', #), ("-#-\'😀 ok\'- #", -#), (\'\\u#bdf\', #))' # wrapped | mm: Err# '((\'\', #), ("-#-\'😀 ok\'- #", -#), (\'௟\', #))' # wrapped
import math
try:
    class Err5(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err5(repr(tuple(sorted({chr(3039): (1 << (0 % 70)), '{}-{!r}-{:>4}'.format(-17, '\U0001f600 ok', 100): (-1 ** (-17 % 7)), 'a,b,,c'.partition('s')[1]: round(3.0)}.items()))), 3)
        except Err5 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
