# difffuzz divergence, seed 25901
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u17fc']
#   mm:      ['៼']
# signature: CPython: ['\u#fc'] | mm: ['៼']
import math
try:
    data = sorted(list(chr(6140)), key=lambda w: (-len(w), w))
    print(sorted(data, key=None, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
