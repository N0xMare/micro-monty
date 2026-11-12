# difffuzz divergence, seed 3738
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0bd1']
#   mm:      ['௑']
# signature: CPython: ['\u#bd#'] | mm: ['௑']
import math
try:
    data = list(chr(3025).expandtabs())
    print(sorted(data, key=lambda q: (str(q), q), reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
