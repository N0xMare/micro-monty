# difffuzz divergence, seed 22665
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v9 ['\u0f6f']
#   mm:      v9 ['཯']
# signature: CPython: v# ['\u#f#f'] | mm: v# ['཯']
import math
try:
    v9 = chr(3951).split(',')
    print('v9', v9)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
