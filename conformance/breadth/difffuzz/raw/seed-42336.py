# difffuzz divergence, seed 42336
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0590']
#   mm:      ['֐']
# signature: CPython: ['\u#'] | mm: ['֐']
import math
try:
    data = list('123'.capitalize()[:4].join(chr(1424).split(',')))
    print(sorted(data, key=lambda q: (str(q), q), reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
