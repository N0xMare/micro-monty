# difffuzz divergence, seed 27700
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0ada']
#   mm:      ['૚']
# signature: CPython: ['\u#ada'] | mm: ['૚']
import math
try:
    print(chr(2778).split(' '))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
