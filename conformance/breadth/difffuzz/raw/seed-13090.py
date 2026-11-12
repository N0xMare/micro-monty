# difffuzz divergence, seed 13090
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0a3d'
#   mm:      EXC ValueError invalid literal for int() with base 10: '਽'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#d' | mm: EXC ValueError invalid literal for int() with base #: '਽'
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i15 in range(5):
        if i15 == 3:
            break
        v16 = {-1}
        def f17(p0):
            data = [i + v for i, v in enumerate(list(filter(lambda q: q > 5, [a * b for a, b in zip(lst0, lst0)]))[:0], 0)]
            def make19(start):
                total = start
                def step(by=1):
                    nonlocal total
                return step
            s = make19(round(-0.0))
            fs = [lambda q, i=i: q * i for i in range(4)]
            return int(p0)
        print('f17(chr(2621))', f17(chr(2621)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
