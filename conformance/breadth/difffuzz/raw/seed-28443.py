# difffuzz divergence, seed 28443
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0ebe']
#   mm:      ['຾']
# signature: CPython: ['\u#ebe'] | mm: ['຾']
import math
try:
    data = list(chr(3774))
    print(sorted(data, key=lambda q: (str(q), q), reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
