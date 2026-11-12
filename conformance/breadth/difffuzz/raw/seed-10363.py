# difffuzz divergence, seed 10363
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0e64'
#   mm:      EXC ValueError invalid literal for int() with base 10: '๤'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '๤'
import math
try:
    n18 = 4
    while n18 > 0:
        for i19 in range(5):
            if isinstance(sum(list(reversed([-17, 0][:-4]))), list):
                print([v6, (max(sorted(v14)) % min(([5] + lst0)))])
            else:
                print([(int(chr(3684)) ** (math.floor(float(i19)) % 7)), max([i + v for i, v in enumerate(([5, 100, 1, 1000, 42] + [-3]), 2)]), 7, -{k20: len(k20) for k20 in ['123', 'a,b,,c', 'Hello World', '123']}['k1']])
            def rec26(n):
                if n <= 1:
                    return n
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
