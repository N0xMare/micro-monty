# difffuzz divergence, seed 26514
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0ba6']
#   mm:      ['஦']
# signature: CPython: ['\u#ba#'] | mm: ['஦']
import math
try:
    print(chr(2982).splitlines(), sep='123')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
