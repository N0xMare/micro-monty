# difffuzz divergence, seed 27629
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u2eff': 7}
#   mm:      {'⻿': 7}
# signature: CPython: {'\u#eff': #} | mm: {'⻿': #}
import math
try:
    lst0 = [3, 1, 2]
    lst0[3:] = sorted({2, 7, 1000})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print({k: v for k, v in zip(sorted(chr(12031).split()[0].splitlines(), key=lambda w: w[::-1]), list(filter(lambda q: q > 4, lst0))) if v})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
