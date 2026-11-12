# difffuzz divergence, seed 11469
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err24 '\u0ad6' 5 wrapped
#   mm:      Err24 '૖' 5 wrapped
# signature: CPython: Err# '\u#ad#' # wrapped | mm: Err# '૖' # wrapped
import math
try:
    n23 = 3
    while n23 > 0:
        class Err24(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err24(chr(2774), 5)
            except Err24 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
            print("finally", divmod(pow(pow((n19 << (0 % 70)), 2, 2 ** 61 - 1), 1, 1000), {'tab\there'.swapcase(): abs(255).bit_length(), '\U0001f600 ok'.lstrip('AbC'): math.ceil(abs(1000) % 30)}['k1'])[0])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
