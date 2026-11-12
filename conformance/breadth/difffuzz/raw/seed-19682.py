# difffuzz divergence, seed 19682
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: walrus EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
#   mm:      EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
# signature: CPython: walrus EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_ma | mm: EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_str_d
import math
try:
    for i21 in [len(w) for w in [str(q) for q in [-1, 1000, 42]]]:
        lst0 = [3, 1, 2]
        def make23(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    if (w := (3 ** 50 ** (([a * b for a, b in zip([7, 100, 100, 10], lst0)] + list(reversed([100, 42, 1, 7])))[len({-17, -17, -3})] % 7))) > 7:
        print("walrus", w)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
