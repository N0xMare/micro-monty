# difffuzz divergence, seed 4958
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v19 ['\u0dce']
#   mm:      v19 ['෎']
# signature: CPython: v# ['\u#dce'] | mm: v# ['෎']
import math
try:
    v19 = chr(3534).split(',')
    print('v19', v19)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
