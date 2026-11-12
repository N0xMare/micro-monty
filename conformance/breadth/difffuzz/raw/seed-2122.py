# difffuzz divergence, seed 2122
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v3 (3, 2, '*\u0bdc*')
#   mm:      v3 (3, 2, '*௜*')
# signature: CPython: v# (#, #, '*\u#bdc*') | mm: v# (#, #, '*௜*')
import math
try:
    v3 = (3, abs(2).bit_length(), chr(3036).center(3, chr(42)), )
    print('v3', v3)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
