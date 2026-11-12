# difffuzz divergence, seed 26786
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0600']
#   mm:      ['؀']
# signature: CPython: ['\u#'] | mm: ['؀']
import math
try:
    data = chr(1536).splitlines()
    print(sorted(data, key=len, reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
