# difffuzz divergence, seed 6406
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u1af8'
#   mm:      EXC ValueError invalid literal for int() with base 2: '᫸'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#af#' | mm: EXC ValueError invalid literal for int() with base #: '᫸'
import math
try:
    if (str(tuple(list({'k0': 3, 'k1': 100}.values()))) in chr(9830)):
        class Ctx24:
            def __init__(self, name):
                self.name = name
    else:
        n25 = 5
        while n25 > 0:
            n25 -= 1
            def make26(start):
                total = start
                def step(by=1):
                    nonlocal total
                return step
            s = make26((math.factorial(+(-2.25 ** 2)) if ((1 << 100) - 1 < round(round(1e10, 1)) <= v4) else pow(max(sorted([-17, -3, 7, -17], key=lambda q: (q % 3, -q))), 34, -5)))
            fs = [lambda q, i=i: q * i for i in range(4)]
            g = (q * 2 for q in sorted({5}))
        v27 = isinstance((int(-0.0) ** (int(chr(6904), 2) % 7)), bool)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
