# difffuzz divergence, seed 385
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['***\u1778***']
#   mm:      ['***᝸***']
# signature: CPython: ['***\u#***'] | mm: ['***᝸***']
import math
try:
    data = chr(6008).center(7, chr(42)).splitlines()
    print(sorted(data, key=lambda q: str(q)[::-1], reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
