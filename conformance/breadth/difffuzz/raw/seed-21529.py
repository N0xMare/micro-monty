# difffuzz divergence, seed 21529
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v16 ['\u242e']
#   mm:      v16 ['␮']
# signature: CPython: v# ['\u#e'] | mm: v# ['␮']
import math
try:
    v16 = chr(9262).splitlines()
    print('v16', v16)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
