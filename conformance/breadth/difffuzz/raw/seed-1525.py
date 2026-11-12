# difffuzz divergence, seed 1525
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v32 ['\u17fa']
#   mm:      v32 ['៺']
# signature: CPython: v# ['\u#fa'] | mm: v# ['៺']
import math
try:
    v32 = sorted(chr(6138).partition('s')[0].splitlines(), key=lambda w: w[::-1])
    print('v32', v32)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
