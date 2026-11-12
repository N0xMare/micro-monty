# difffuzz divergence, seed 9953
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u175b': 8}
#   mm:      {'᝛': 8}
# signature: CPython: {'\u#b': #} | mm: {'᝛': #}
import math
try:
    print({k: v for k, v in zip(chr(5979).split(' ')[0].zfill(0).splitlines(), [(sum(sorted({s2 % 7 for s2 in [100, 2, 1000, 100]})) + i1) for i1 in range(7)]) if v})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
