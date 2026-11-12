# difffuzz divergence, seed 12103
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u20c3']
#   mm:      ['⃃']
# signature: CPython: ['\u#c#'] | mm: ['⃃']
import math
try:
    data = chr(8387).split(' ')
    print(sorted(data, key=None, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
