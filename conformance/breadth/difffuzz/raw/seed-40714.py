# difffuzz divergence, seed 40714
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u07fc', '1', '2', '3']
#   mm:      ['߼', '1', '2', '3']
# signature: CPython: ['\u#fc', '#', '#', '#'] | mm: ['߼', '#', '#', '#']
import math
try:
    data = list((chr(2044) + '123'.zfill(3).upper()))
    print(sorted(data, key=len, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
