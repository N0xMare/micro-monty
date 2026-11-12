# difffuzz divergence, seed 43542
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'**\u0600': 3, 'z': 1} [('42', 42), -14, 1, 7, 10, 10, 16, 44, 47, 255]
#   mm:      {'**؀': 3, 'z': 1} [('42', 42), -14, 1, 7, 10, 10, 16, 44, 47, 255]
# signature: CPython: {'**\u#': #, 'z': #} [('#', #), -#, #, #, #, #, #, #, #, #] | mm: {'**؀': #, 'z': #} [('#', #), -#, #, #, #, #, #, #, #, #]
import math
try:
    print({**{k14: len(k14) for k14 in chr(1536).rjust(3, chr(42)).splitlines()}, "z": 1}, [*tuple(sorted({str(q): q for q in (sorted({42}) * 1)}.items())), *sorted(sorted((list(filter(lambda q: q > 5, [7, -3, 255, 10, 10])) + [i + v for i, v in enumerate([42, -17, -3, 42, 10], 2)]), reverse=True))])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
