# difffuzz divergence, seed 1543
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v2 ['\u19dc']
#   mm:      v2 ['᧜']
# signature: CPython: v# ['\u#dc'] | mm: v# ['᧜']
import math
try:
    v2 = list(chr(6620))
    print('v2', v2)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
