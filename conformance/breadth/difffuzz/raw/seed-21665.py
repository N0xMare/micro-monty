# difffuzz divergence, seed 21665
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['k0\u0e70k1\u0e70k2']
#   mm:      ['k0๰k1๰k2']
# signature: CPython: ['k#\u#e#k#\u#e#k#'] | mm: ['k#๰k#๰k#']
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    if (not any(list(filter(lambda q: q > 2, [1000, 100, 5, -3][:])))):
        print(lst0)
    else:
        print(sorted(chr(3696).join(list({'k0': 10, 'k1': -1, 'k2': 42})).splitlines(), key=str.lower))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
