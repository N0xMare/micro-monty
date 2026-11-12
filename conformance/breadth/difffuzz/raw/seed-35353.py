# difffuzz divergence, seed 35353
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0bab']
#   mm:      ['஫']
# signature: CPython: ['\u#bab'] | mm: ['஫']
import math
try:
    print(chr(2987)[3::-1].split(','), sep='  pad  ')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
