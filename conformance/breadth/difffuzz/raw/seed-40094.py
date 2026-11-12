# difffuzz divergence, seed 40094
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'000False\u06dd': 44}-42[3, 1, 2]
#   mm:      {'000False۝': 44}-42[3, 1, 2]
# signature: CPython: {'#False\u#dd': #}-#[#, #, #] | mm: {'#False۝': #}-#[#, #, #]
import math
try:
    v9 = (('%d' % (0.1,) + str(False)).zfill(8) + chr(1757))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(dict(zip(sorted(v9.center(4, chr(42)).splitlines(), key=len), [i + v for i, v in enumerate([i + v for i, v in enumerate(list(reversed([42])), 2)], 0)])), sep='\U0001f600 ok', end='-42')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    lst0 = [3, 1, 2]
    print(lst0)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
