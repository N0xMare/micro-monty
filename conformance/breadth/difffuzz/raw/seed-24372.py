# difffuzz divergence, seed 24372
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '0\u2fea'
#   mm:      EXC ValueError could not convert string to float: '0⿪'
# signature: CPython: EXC ValueError could not convert string to float: '#\u#fea' | mm: EXC ValueError could not convert string to float: '#⿪'
import math
try:
    n18 = 5
    while n18 > 0:
        lst0 = [3, 1, 2]
        def f20(p0, p1, p2):
            def rec21(n):
                if n <= 1:
                    return n
            return str(abs(float(p2))).rjust(0, chr(45))
        print("f20(sorted([a * b for a, b in zip([i + v for i, v in enumerate((lst0 * 2), 0)], list({'k0': 1000}.values()))], key=lambda q: (q % 3, -q)), [i + v for i, v in enumerate(sorted(list(map(lambda q: q * 2 + 1, (lst0 * 2))), reverse=True), 1)], chr(12266).zfill(2).swapcase())", f20(sorted([a * b for a, b in zip([i + v for i, v in enumerate((lst0 * 2), 0)], list({'k0': 1000}.values()))], key=lambda q: (q % 3, -q)), [i + v for i, v in enumerate(sorted(list(map(lambda q: q * 2 + 1, (lst0 * 2))), reverse=True), 1)], chr(12266).zfill(2).swapcase()))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
