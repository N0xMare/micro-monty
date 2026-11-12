# difffuzz divergence, seed 27560
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u07bb']
#   mm:      ['޻']
# signature: CPython: ['\u#bb'] | mm: ['޻']
import math
try:
    data = chr(1979).split(None)[0].strip().splitlines()
    print(sorted(data, key=lambda q: str(q)[::-1], reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
