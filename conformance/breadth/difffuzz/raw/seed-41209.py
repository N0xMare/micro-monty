# difffuzz divergence, seed 41209
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u2e71']
#   mm:      ['⹱']
# signature: CPython: ['\u#e#'] | mm: ['⹱']
import math
try:
    data = sorted(list(chr(11889)), key=lambda w: w[::-1])
    print(sorted(data, key=lambda q: (str(q), q), reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
