# difffuzz divergence, seed 6363
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u17ff']
#   mm:      ['៿']
# signature: CPython: ['\u#ff'] | mm: ['៿']
import math
try:
    data = chr(6143).split()
    print(sorted(data, key=str, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
