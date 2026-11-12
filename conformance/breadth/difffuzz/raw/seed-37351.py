# difffuzz divergence, seed 37351
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u1ccd']
#   mm:      ['᳍']
# signature: CPython: ['\u#ccd'] | mm: ['᳍']
import math
try:
    print(list(chr(7373)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
