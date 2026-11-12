# difffuzz divergence, seed 41712
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: yv14 ['\u12c6']
#   mm:      yv14 ['዆']
# signature: CPython: yv# ['\u#c#'] | mm: yv# ['዆']
import math
try:
    print((1000, 2, ), end='x\ny')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v14 = list(chr(4806))
    print('v14', v14)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
