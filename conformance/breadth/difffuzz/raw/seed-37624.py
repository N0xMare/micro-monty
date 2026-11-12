# difffuzz divergence, seed 37624
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u20f5']
#   mm:      ['⃵']
# signature: CPython: ['\u#f#'] | mm: ['⃵']
import math
try:
    data = list(chr(8437))
    print(sorted(data, key=str, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
