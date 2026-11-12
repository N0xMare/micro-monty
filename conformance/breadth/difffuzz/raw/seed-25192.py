# difffuzz divergence, seed 25192
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u2d72']
#   mm:      ['⵲']
# signature: CPython: ['\u#d#'] | mm: ['⵲']
import math
try:
    print(list(chr(11634)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
