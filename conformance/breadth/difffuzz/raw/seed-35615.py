# difffuzz divergence, seed 35615
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v14 ['\u0fe3']
#   mm:      v14 ['࿣']
# signature: CPython: v# ['\u#fe#'] | mm: v# ['࿣']
import math
try:
    v14 = list(chr(4067))
    print('v14', v14)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
