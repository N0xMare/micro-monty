# difffuzz divergence, seed 9175
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: many EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
#   mm:      EXC ValueError Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit
# signature: CPython: many EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_ | mm: EXC ValueError Exceeds the limit (# digits) for integer string conversion; use sys.set_int_max_str_d
import math
try:
    def f2(p0, p1):
        match tuple([((3 ** 50 ** (math.floor(abs(p0) % 30) % 7)) + i3) for i3 in range(-3, 9)]):
            case 'a' | 'hello':
                print("word")
            case [x, y, *rest]:
                print("many", x, y, rest)
    print('f2(-3, list(dict(sorted({k6: len(k6) for k6 in [str(q) for q in [-17]]}.items(), key=lambda kv: kv[1])).values()))', f2(-3, list(dict(sorted({k6: len(k6) for k6 in [str(q) for q in [-17]]}.items(), key=lambda kv: kv[1])).values())))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
