# difffuzz divergence, seed 24331
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err18 42-'\u0e83'-  -3 9 wrapped
#   mm:      Err18 42-'຃'-  -3 9 wrapped
# signature: CPython: Err# #-'\u#e#'-  -# # wrapped | mm: Err# #-'຃'-  -# # wrapped
import math
try:
    n15 = 1
    while n15 > 0:
        n15 -= 1
        n16 = 1
        while n16 > 0:
            for i17 in range(1, 8):
                if i17 == 3:
                    continue
                lst0 = [3, 1, 2]
            class Err18(ValueError):
                def __init__(self, msg, code=0):
                    super().__init__(msg)
                    self.code = code
            try:
                try:
                    raise Err18(('{}-{!r}-{:>4}'.format(42, chr(3715), -3) * ((-(10 ** 25) - (-n16 ** (abs(n15).bit_length() % 7))) % 4)), 9)
                except Err18 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
            if (chr(8902).join(v11.splitlines()).center(7, chr(45)) <= repr({'k0': -3, 'k1': 10})):
                print(lst0)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
