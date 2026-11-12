# difffuzz divergence, seed 31305
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u2434']
#   mm:      ['␴']
# signature: CPython: ['\u#'] | mm: ['␴']
import math
try:
    data = sorted(sorted(chr(9268).splitlines(), key=lambda w: w[::-1]), key=str.lower)
    print(sorted(data, key=lambda q: (str(q), q), reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
