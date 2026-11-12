# difffuzz divergence, seed 24690
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u2d6e'
#   mm:      EXC ValueError could not convert string to float: '⵮'
# signature: CPython: EXC ValueError could not convert string to float: '\u#d#e' | mm: EXC ValueError could not convert string to float: '⵮'
import math
try:
        v3 = set(list(map(lambda q: q * 2 + 1, []))[-5:6][:6:-2])
        v4 = chr(11630)
        class Ctx5:
            def __init__(self, name):
                self.name = name
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f9(p0, p1):
        match list({k11: len(k11) for k11 in v4[p0].splitlines()}.values()):
            case list(l):
                print("list", len(l))
    print('f9(int((-float(v4) ** 3)), [a * b for a, b in zip(sorted({2}), list(map(lambda q: q * 3 + 1, list(filter(lambda q: q > -1, (lst0 * 2))))))])', f9(int((-float(v4) ** 3)), [a * b for a, b in zip(sorted({2}), list(map(lambda q: q * 3 + 1, list(filter(lambda q: q > -1, (lst0 * 2))))))]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
