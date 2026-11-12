# difffuzz divergence, seed 42410
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0a3d']
#   mm:      ['਽']
# signature: CPython: ['\u#a#d'] | mm: ['਽']
import math
try:
    data = chr(2621).splitlines()
    print(sorted(data, key=str, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
