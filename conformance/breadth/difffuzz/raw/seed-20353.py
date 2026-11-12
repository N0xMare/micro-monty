# difffuzz divergence, seed 20353
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u125e']
#   mm:      ['቞']
# signature: CPython: ['\u#e'] | mm: ['቞']
import math
try:
    print(chr(4702).strip('x\ny'[3:4][:]).split(' '), sep='  pad  ')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
