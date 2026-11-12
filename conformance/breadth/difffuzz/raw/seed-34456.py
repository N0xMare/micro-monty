# difffuzz divergence, seed 34456
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC TypeError not all arguments converted during string formatting
#   mm:      EXC MemoryError 
# signature: CPython: EXC TypeError not all arguments converted during string formatting | mm: EXC MemoryError 
import math
try:
    class Err17(ValueError):
        def __init__(self, msg, code=0):
            self.code = code
    try:
        try:
            raise Err17('%%%d' % ((1 << 100) - 1, math.factorial(abs((2 ** 64 ** (round(-2.25) % 7))) % 30),), 9)
        except Err17 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
