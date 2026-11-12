# difffuzz divergence, seed 28465
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u2073']
#   mm:      ['⁳']
# signature: CPython: ['\u#'] | mm: ['⁳']
import math
try:
    n8 = 2
    while n8 > 0:
        n8 -= 1
        def rec9(n):
            if n <= 1:
                return n
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    data = list((chr(8307) * (((10 << (n8 % 70)) ** ([0, 7, 1000, 42][n8] % 7)) % 4)))
    print(sorted(data, key=str, reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
