# difffuzz divergence, seed 2948
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u139e']
#   mm:      ['᎞']
# signature: CPython: ['\u#e'] | mm: ['᎞']
import math
try:
    data = chr(5022).ljust(0, chr(42)).splitlines()
    print(sorted(data, key=lambda q: (str(q), q), reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
