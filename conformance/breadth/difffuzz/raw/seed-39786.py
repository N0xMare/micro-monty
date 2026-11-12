# difffuzz divergence, seed 39786
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0e70']
#   mm:      ['๰']
# signature: CPython: ['\u#e#'] | mm: ['๰']
import math
try:
    data = list(chr(3696))
    print(sorted(data, key=lambda q: (str(q), q), reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
