# difffuzz divergence, seed 13832
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0af4']
#   mm:      ['૴']
# signature: CPython: ['\u#af#'] | mm: ['૴']
import math
try:
    data = chr(2804).split(' ')
    print(sorted(data, key=lambda q: str(q)[::-1], reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
