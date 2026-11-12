# difffuzz divergence, seed 698
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['⢘\u2d7b']
#   mm:      ['⢘⵻']
# signature: CPython: ['⢘\u#d#b'] | mm: ['⢘⵻']
import math
try:
    data = (chr(10392).replace('AbC'.strip('3.5'), ('123' * (1000 % 4))) + chr(11643)).splitlines()
    print(sorted(data, key=str, reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
