# difffuzz divergence, seed 42020
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: [1, 7]  pad  Err1 '000\u0d11' 5 wrapped
#   mm:      [1, 7]  pad  Err1 '000഑' 5 wrapped
# signature: CPython: [#, #]  pad  Err# '#\u#d#' # wrapped | mm: [#, #]  pad  Err# '#഑' # wrapped
import math
try:
    print(list({'k0': 7, 'k1': 1}.values())[5::-1], end='  pad  ')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class Err1(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err1(chr(3345).zfill(4), 5)
        except Err1 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
