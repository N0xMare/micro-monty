# difffuzz divergence, seed 8848
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u0ee2'
#   mm:      EXC ValueError invalid literal for int() with base 2: '໢'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ee#' | mm: EXC ValueError invalid literal for int() with base #: '໢'
import math
try:
    if ((int(chr(3810), 2) < len(list({'k0': 0}.values())) <= pow(sum(v8), 26, -5)) or (divmod(len([-3, 1]), round(1e-05))[0] < min([i + v for i, v in enumerate(v8, 1)]) <= dict(zip([], []))['k2'])):
        print(v8)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
