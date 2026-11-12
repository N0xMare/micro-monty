# difffuzz divergence, seed 18807
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u1a8b']
#   mm:      ['᪋']
# signature: CPython: ['\u#a#b'] | mm: ['᪋']
import math
try:
    data = chr(6795).split()
    print(sorted(data, key=None, reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
