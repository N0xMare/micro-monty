# difffuzz divergence, seed 6672
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: finally EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
#   mm:      EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
# signature: CPython: finally EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_m | mm: EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_str_d
import math
try:
    try:
        print('Hello World' + list(reversed(([(1000 + i2) for i2 in range(3, 8) if i2 % 3] + sorted({10}))))[divmod(((1000 if False else 255) ** (math.isqrt(abs(5) % 30) % 7)), +(10 << (-1 % 70)))[1]])
    except IndexError as e:
        print("caught", type(e).__name__, e, e.args)
        print("finally", (3 ** 50 ** (-3 % 7)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
