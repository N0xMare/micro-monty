# difffuzz divergence, seed 19750
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v18 ['\u0ff5']
#   mm:      v18 ['࿵']
# signature: CPython: v# ['\u#ff#'] | mm: v# ['࿵']
import math
try:
    v18 = list(chr(4085))
    print('v18', v18)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
