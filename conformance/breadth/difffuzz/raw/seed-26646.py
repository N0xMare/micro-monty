# difffuzz divergence, seed 26646
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u18ff']
#   mm:      ['᣿']
# signature: CPython: ['\u#ff'] | mm: ['᣿']
import math
try:
    print(sorted(list(chr(6399)), key=str.lower))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
