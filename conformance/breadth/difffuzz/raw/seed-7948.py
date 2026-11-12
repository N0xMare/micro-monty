# difffuzz divergence, seed 7948
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0fbd']
#   mm:      ['྽']
# signature: CPython: ['\u#fbd'] | mm: ['྽']
import math
try:
    print(chr(4029).splitlines(), sep='-42')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
