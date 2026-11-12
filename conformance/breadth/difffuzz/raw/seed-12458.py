# difffuzz divergence, seed 12458
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v17 ['\u0cfc']
#   mm:      v17 ['೼']
# signature: CPython: v# ['\u#cfc'] | mm: v# ['೼']
import math
try:
    for i16 in range(1):
        if i16 == 3:
            continue
        v17 = list(('\u00df' + '\u00df').removeprefix(chr(11948)).join(chr(3324).split()))
        print('v17', v17)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
