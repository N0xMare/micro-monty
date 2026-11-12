# difffuzz divergence, seed 44170
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v8 ['\u1a9e']
#   mm:      v8 ['᪞']
# signature: CPython: v# ['\u#a#e'] | mm: v# ['᪞']
import math
try:
    v8 = sorted(sorted(list(chr(6814)), key=lambda w: w[::-1]), key=lambda w: w[::-1])
    print('v8', v8)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
