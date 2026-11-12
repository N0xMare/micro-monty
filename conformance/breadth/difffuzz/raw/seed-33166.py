# difffuzz divergence, seed 33166
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
#   mm:      EXC MemoryError 
# signature: CPython: EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_str_d | mm: EXC MemoryError 
import math
try:
    def make4(start):
        total = start
        def step(by=1):
            nonlocal total
            return total
        return step
    s = make4((2 ** 64 ** (-(2 ** 70) % 7)))
    print(s(), s(0), s(by=0))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
