# difffuzz divergence, seed 7592
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v25 ['\u173b']
#   mm:      v25 ['᜻']
# signature: CPython: v# ['\u#b'] | mm: v# ['᜻']
import math
try:
    v25 = chr(5947).splitlines()
    print('v25', v25)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
