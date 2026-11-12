# difffuzz divergence, seed 38563
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u19ce']tab	hereTrue [0]
#   mm:      ['᧎']tab	hereTrue [0]
# signature: CPython: ['\u#ce']tab	hereTrue [#] | mm: ['᧎']tab	hereTrue [#]
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(sorted(chr(6606).splitlines(), key=str.lower), end='tab\there')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    x = y = [a * b for a, b in zip(sorted(lst0), sorted({s7 % 6 for s7 in [a * b for a, b in zip([], lst0)]}))]
    x.append(0)
    print(x is y, y)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
