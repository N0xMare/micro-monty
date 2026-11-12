# difffuzz divergence, seed 831
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v7 ['\u17fe']
#   mm:      v7 ['៾']
# signature: CPython: v# ['\u#fe'] | mm: v# ['៾']
import math
try:
    v7 = chr(6142).splitlines()
    print('v7', v7)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
