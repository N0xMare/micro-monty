# difffuzz divergence, seed 1687
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u1774']
#   mm:      ['᝴']
# signature: CPython: ['\u#'] | mm: ['᝴']
import math
try:
    print(chr(6004).split(), sep='Hello World')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
