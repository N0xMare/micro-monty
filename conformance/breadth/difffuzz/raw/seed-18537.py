# difffuzz divergence, seed 18537
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u2dd7'
#   mm:      EXC ValueError could not convert string to float: '⷗'
# signature: CPython: EXC ValueError could not convert string to float: '\u#dd#' | mm: EXC ValueError could not convert string to float: '⷗'
import math
try:
    def f5(p0, p1, p2):
        if str(float(chr(11735))).isupper():
            def rec7(n):
                if n <= 1:
                    return n
            match divmod(round(v6), (len(list({}.values())) << (10 ** 30 + 7 % 70))):
                case [x]:
                    print("one", x)
            if (w := ((3 >> (min(list({'k0': 42, 'k1': 2, 'k2': -3, 'k3': 100}.values())) % 9)) ** (~int(float('Hello World')) % 7))) > 17:
                print("walrus", w)
            total = start
            def step(by=1):
                return total
    print('f5(list(range(-1, -1, -2)), list(filter(lambda q: q > 1, list(range(3, 8, 1)))), list(reversed((([5] + sorted({-17, 3, 3})) + ([(10 + i12) for i12 in []] + [3, 1000, -1, 5, 7][:])))))', f5(list(range(-1, -1, -2)), list(filter(lambda q: q > 1, list(range(3, 8, 1)))), list(reversed((([5] + sorted({-17, 3, 3})) + ([(10 + i12) for i12 in []] + [3, 1000, -1, 5, 7][:]))))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
