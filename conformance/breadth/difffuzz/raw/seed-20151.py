# difffuzz divergence, seed 20151
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: walrus EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
#   mm:      EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
# signature: CPython: walrus EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_ma | mm: EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_str_d
import math
try:
    if (w := (3 ** 50 ** (math.floor(abs(min(sorted({10}))) % 30) % 7))) > -2:
        print("walrus", w)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
