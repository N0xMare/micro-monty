# difffuzz divergence, seed 4539
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0a7b']
#   mm:      ['੻']
# signature: CPython: ['\u#a#b'] | mm: ['੻']
import math
try:
        data = chr(2683).split(',')
        print(sorted(data, key=None, reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
