# difffuzz divergence, seed 9087
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: finally EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
#   mm:      EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
# signature: CPython: finally EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_m | mm: EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_str_d
import math
try:
    def risky():
        try:
            print('a'.split('s')[-1].swapcase().replace(repr(sorted(['Hello World', '3.5', '-42'], key=lambda w: w[::-1])), f'{None:>8}{255:b} {1e-05:08.2f}|')[:] + 100)
        finally:
            print("finally", (3 ** 50 ** (len(({2, 7, 7, 2} ^ {s2 % 2 for s2 in [3, 0, 3]})) % 7)))
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
