# difffuzz divergence, seed 18050
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v20 EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
#   mm:      EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
# signature: CPython: v# EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_st | mm: EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_str_d
import math
try:
    v2 = {'k0': 100, 'k1': 255}
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v20 = (2 ** 64 ** ((dict(sorted(v2.items(), key=lambda kv: kv[1])) | dict(sorted(v2.items(), key=lambda kv: kv[1])))['k1'] % 7))
    print('v20', v20)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
