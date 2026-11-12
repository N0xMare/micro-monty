# difffuzz divergence, seed 29186
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u07b2']
#   mm:      ['޲']
# signature: CPython: ['\u#b#'] | mm: ['޲']
import math
try:
    data = list(chr(1970))
    print(sorted(data, key=lambda q: (str(q), q), reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
