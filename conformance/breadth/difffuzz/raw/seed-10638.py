# difffuzz divergence, seed 10638
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v37 ['\u0b50']
#   mm:      v37 ['୐']
# signature: CPython: v# ['\u#b#'] | mm: v# ['୐']
import math
try:
    v37 = chr(2896).split()
    print('v37', v37)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
