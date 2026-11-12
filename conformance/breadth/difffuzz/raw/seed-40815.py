# difffuzz divergence, seed 40815
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v14 ['\u2457']
#   mm:      v14 ['⑗']
# signature: CPython: v# ['\u#'] | mm: v# ['⑗']
import math
try:
    v14 = sorted(chr(9303).splitlines(), key=len)
    print('v14', v14)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
