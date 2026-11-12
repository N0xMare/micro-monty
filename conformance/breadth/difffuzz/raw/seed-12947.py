# difffuzz divergence, seed 12947
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v28 ['\u0a11']
#   mm:      v28 ['਑']
# signature: CPython: v# ['\u#a#'] | mm: v# ['਑']
import math
try:
    v28 = list({chr(2577): (7 if '123'.isalnum() else (n18 & n18))})
    print('v28', v28)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
