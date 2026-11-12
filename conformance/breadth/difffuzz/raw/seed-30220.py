# difffuzz divergence, seed 30220
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err1 'hello\u07behello' 5 wrapped
#   mm:      Err1 'hello޾hello' 5 wrapped
# signature: CPython: Err# 'hello\u#behello' # wrapped | mm: Err# 'hello޾hello' # wrapped
import math
try:
    class Err1(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err1(chr(1982).replace('', 'hello'.strip().lower().replace(('3.5' + 'tab\there').join(list('caf\u00e9')), ('%%%d' % (-2.25,) * ('caf\u00e9'.rfind('hello') % 4)))), 5)
        except Err1 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
