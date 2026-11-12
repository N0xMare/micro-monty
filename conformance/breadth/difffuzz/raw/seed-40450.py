# difffuzz divergence, seed 40450
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u088f']
#   mm:      ['࢏']
# signature: CPython: ['\u#f'] | mm: ['࢏']
import math
try:
    data = sorted(chr(2191).title().splitlines(), key=lambda w: w[::-1])
    print(sorted(data, key=None, reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
