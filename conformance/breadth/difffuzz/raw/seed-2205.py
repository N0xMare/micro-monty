# difffuzz divergence, seed 2205
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0b3b']
#   mm:      ['଻']
# signature: CPython: ['\u#b#b'] | mm: ['଻']
import math
try:
    data = chr(2875).zfill(0).split(' ')
    print(sorted(data, key=None, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
