# difffuzz divergence, seed 40793
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u175c']
#   mm:      ['᝜']
# signature: CPython: ['\u#c'] | mm: ['᝜']
import math
try:
    data = sorted(list((''.join([]) + chr(5980))), key=len)
    print(sorted(data, key=str, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
