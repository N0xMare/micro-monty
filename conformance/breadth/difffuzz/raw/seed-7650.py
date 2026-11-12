# difffuzz divergence, seed 7650
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError not enough values to unpack (expected 2, got 1)
#   mm:      EXC MemoryError 
# signature: CPython: EXC ValueError not enough values to unpack (expected #, got #) | mm: EXC MemoryError 
import math
try:
    n21 = 6
    while n21 > 0:
        (a, b), c = ((42, )[-6:] + tuple([i + v for i, v in enumerate(sorted([], key=lambda q: (q % 3, -q)), 2)])), (2 ** 64 ** (abs(math.ceil((2.5 ** 3))).bit_length() % 7))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
