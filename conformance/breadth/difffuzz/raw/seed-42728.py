# difffuzz divergence, seed 42728
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0890']
#   mm:      ['࢐']
# signature: CPython: ['\u#'] | mm: ['࢐']
import math
try:
    data = chr(2192).split(' ')
    print(sorted(data, key=lambda q: str(q)[::-1], reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
