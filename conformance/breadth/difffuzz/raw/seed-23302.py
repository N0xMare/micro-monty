# difffuzz divergence, seed 23302
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0cfc']
#   mm:      ['೼']
# signature: CPython: ['\u#cfc'] | mm: ['೼']
import math
try:
    print(list(chr(3324)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
