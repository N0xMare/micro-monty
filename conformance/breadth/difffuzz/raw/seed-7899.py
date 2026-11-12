# difffuzz divergence, seed 7899
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0e61']
#   mm:      ['๡']
# signature: CPython: ['\u#e#'] | mm: ['๡']
import math
try:
    data = list(chr(3681).removeprefix('x\ny'.swapcase()[::-1]))
    print(sorted(data, key=None, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
