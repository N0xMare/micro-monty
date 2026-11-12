# difffuzz divergence, seed 13233
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: 100000-'\u2439'-  -5
#   mm:      100000-'␹'-  -5
# signature: CPython: #-'\u#'-  -# | mm: #-'␹'-  -#
import math
try:
    for i1 in range(7):
        if i1 == 1:
            break
        v2 = [0, 100, 5]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n10 = 4
    while n10 > 0:
        class C14:
            def __init__(self, v, w=0):
                self.w = w
        print(a.nope)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
    print('{}-{!r}-{:>4}'.format([a * b for a, b in zip([42, 1000, 1000], v2)][~1], chr(9273), ~n10).zfill(3))
