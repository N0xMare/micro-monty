# difffuzz divergence, seed 3930
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u135c']
#   mm:      ['፜']
# signature: CPython: ['\u#c'] | mm: ['፜']
import math
try:
    print(chr(4956).split(' '))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
