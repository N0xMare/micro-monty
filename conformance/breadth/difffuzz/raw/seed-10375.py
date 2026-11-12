# difffuzz divergence, seed 10375
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v37 ['\u0df9']
#   mm:      v37 ['෹']
# signature: CPython: v# ['\u#df#'] | mm: v# ['෹']
import math
try:
    v37 = sorted(list(chr(3577)), key=str.lower)
    print('v37', v37)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
