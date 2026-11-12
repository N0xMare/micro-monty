# difffuzz divergence, seed 34506
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u2e69']
#   mm:      ['⹩']
# signature: CPython: ['\u#e#'] | mm: ['⹩']
import math
try:
    print(chr(11881).split(' '))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
