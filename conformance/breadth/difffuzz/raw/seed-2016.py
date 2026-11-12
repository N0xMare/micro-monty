# difffuzz divergence, seed 2016
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err20 '\u2dbf' 0 wrapped
#   mm:      Err20 '⶿' 0 wrapped
# signature: CPython: Err# '\u#dbf' # wrapped | mm: Err# '⶿' # wrapped
import math
try:
        class Err20(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err20(chr(11711), 0)
            except Err20 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
            def __init__(self, msg, code=0):
                self.code = code
            try:
                raise Err21('{}-{!r}-{:>4}'.format('caf\u00e9'[i19].count(('' + 'tab\there')), f'{[-3, 0, 5, 5, 10]} and {None:10.3}'.upper(), '3.5'.zfill(1).find(repr(1e300))).capitalize(), 5)
            except Err21 as inner:
                raise RuntimeError("wrapped") from inner
        def rec22(n):
            if n <= 1:
                return n
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
