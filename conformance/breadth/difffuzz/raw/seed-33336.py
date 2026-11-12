# difffuzz divergence, seed 33336
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v19 EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
#   mm:      EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
# signature: CPython: v# EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_st | mm: EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_str_d
import math
try:
    for i18 in range(1):
        if i18 == 4:
            continue
        v19 = {str(1e-05).join(list({'k0': -1, 'k1': 0})).upper(): (2 ** 64 ** (max([5, -3, 10, 3]) % 7)), 'Hello World'[-2:-3]: round(round(float(-1), 2))}
        print('v19', v19)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
