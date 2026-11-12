# difffuzz divergence, seed 37178
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1316'
#   mm:      EXC ValueError invalid literal for int() with base 10: '጖'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '጖'
import math
try:
    lst0 = [3, 1, 2]
    print(lst0.pop())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v10 = min([i + v for i, v in enumerate(list(range(-1, 1, 3)), 0)])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def make11(start):
        total = start
        def step(by=1):
            return total
    s = make11(int((chr(4886) * (lst0[v10] % 4)).expandtabs(), 10))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
