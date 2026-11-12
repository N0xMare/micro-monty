# difffuzz divergence, seed 18791
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v15 ['\u0ad3']
#   mm:      v15 ['૓']
# signature: CPython: v# ['\u#ad#'] | mm: v# ['૓']
import math
try:
    v15 = chr(2771).splitlines()
    print('v15', v15)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
