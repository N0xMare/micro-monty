# difffuzz divergence, seed 4356
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err5 '-17\u177e5\u177e2' 9 wrapped
#   mm:      Err5 '-17᝾5᝾2' 9 wrapped
# signature: CPython: Err# '-#\u#e#\u#e#' # wrapped | mm: Err# '-#᝾#᝾#' # wrapped
import math
try:
    lst0 = [3, 1, 2]
    lst0.extend(((list(dict(zip(['  pad  '], lst0)).values()) + (list(range(5, 8, 2)) * 1)) + list({'k0': -17, 'k1': 100}.values())[:-1][::-1]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class Err5(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err5(chr(6014).join([str(q) for q in list(reversed(lst0[-5::2]))]), 9)
        except Err5 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
