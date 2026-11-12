# difffuzz divergence, seed 38693
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0a11']
#   mm:      ['਑']
# signature: CPython: ['\u#a#'] | mm: ['਑']
import math
try:
    print(chr(2577).splitlines())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
