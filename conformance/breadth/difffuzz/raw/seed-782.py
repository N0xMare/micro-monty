# difffuzz divergence, seed 782
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u0cd9': 1}
#   mm:      {'೙': 1}
# signature: CPython: {'\u#cd#': #} | mm: {'೙': #}
import math
try:
    print({k: v for k, v in zip(list(chr(3289).expandtabs()), [len(w) for w in '\U0001f600 ok'.zfill(7)[4::3].split(',')]) if v})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
