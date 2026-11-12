# difffuzz divergence, seed 34980
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v1 EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
#   mm:      EXC MemoryError 
# signature: CPython: v# EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_st | mm: EXC MemoryError 
import math
try:
    v1 = divmod((2 ** 64 ** ((-(2 ** 70) if bool(10) else divmod(0, -17)[0]) % 7)), sum([(-3 + i2) for i2 in range(4) if i2 % 4]))
    print('v1', v1)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
