# difffuzz divergence, seed 2972
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v22 ['\u07b7']
#   mm:      v22 ['޷']
# signature: CPython: v# ['\u#b#'] | mm: v# ['޷']
import math
try:
    v22 = list({k23: len(k23) for k23 in list(chr(1975))})
    print('v22', v22)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
