# difffuzz divergence, seed 9123
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u2b75']
#   mm:      ['⭵']
# signature: CPython: ['\u#b#'] | mm: ['⭵']
import math
try:
    for i14 in list({'tab\there'.partition(',')[2]: pow((1 << 100) - 1, 12, 2 ** 61 - 1)}.values()):
        if i14 == 5:
            break
        print(chr(11125).splitlines())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
