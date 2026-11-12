# difffuzz divergence, seed 7721
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0cff']
#   mm:      ['೿']
# signature: CPython: ['\u#cff'] | mm: ['೿']
import math
try:
    print(list(chr(3327)), sep='Hello World')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
