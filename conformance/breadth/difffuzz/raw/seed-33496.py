# difffuzz divergence, seed 33496
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v2 ['\u1b4d']
#   mm:      v2 ['᭍']
# signature: CPython: v# ['\u#b#d'] | mm: v# ['᭍']
import math
try:
    v2 = sorted(chr(6989).strip(('a' + 'tab\there')).splitlines(), key=lambda w: w[::-1])
    print('v2', v2)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
