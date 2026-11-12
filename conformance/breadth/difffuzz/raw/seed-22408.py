# difffuzz divergence, seed 22408
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v17 ['\u0adc']
#   mm:      v17 ['૜']
# signature: CPython: v# ['\u#adc'] | mm: v# ['૜']
import math
try:
    v17 = list(chr(2780))
    print('v17', v17)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
