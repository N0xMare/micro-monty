# difffuzz divergence, seed 31041
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'a': 1, 'ß': 1, '\u0dbf': 0, 'z': 1} [('-17', 1), ('0', 2), 1, 1]
#   mm:      {'a': 1, 'ß': 1, '඿': 0, 'z': 1} [('-17', 1), ('0', 2), 1, 1]
# signature: CPython: {'a': #, 'ß': #, '\u#dbf': #, 'z': #} [('-#', #), ('#', #), #, #] | mm: {'a': #, 'ß': #, '඿': #, 'z': #} [('-#', #), ('#', #), #, #]
import math
try:
    n19 = 5
    while n19 > 0:
        n19 -= 1
        v20 = {42}
        lst0 = [3, 1, 2]
        def gen22(n):
                    yield k * 0
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print({**({k29: len(k29) for k29 in ['a', '\u00df']} | {chr(3519): int(round(-0.0, 3))}), "z": 1}, [*tuple(sorted(dict(zip([str(q) for q in list({'k0': 0, 'k1': -17}.values())], (list(reversed(lst0)) * 1))).items())), *list(map(lambda q: q * 0 + 1, (lst0 * 1)[5::-2]))[:]])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
