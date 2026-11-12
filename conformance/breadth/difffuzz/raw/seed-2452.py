# difffuzz divergence, seed 2452
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u1979']
#   mm:      ['᥹']
# signature: CPython: ['\u#'] | mm: ['᥹']
import math
try:
    for i9 in range(2):
        if i9 == 5:
            break
        print(list(chr(6521)), sep='123')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
