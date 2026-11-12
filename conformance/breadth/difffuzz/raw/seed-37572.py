# difffuzz divergence, seed 37572
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v21 ['\u09d1']
#   mm:      v21 ['৑']
# signature: CPython: v# ['\u#d#'] | mm: v# ['৑']
import math
try:
    v21 = chr(2513).splitlines()
    print('v21', v21)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
