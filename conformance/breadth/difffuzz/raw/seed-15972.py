# difffuzz divergence, seed 15972
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u1af8']
#   mm:      ['᫸']
# signature: CPython: ['\u#af#'] | mm: ['᫸']
import math
try:
    print(sorted(chr(6904).splitlines(), key=lambda w: (-len(w), w)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
