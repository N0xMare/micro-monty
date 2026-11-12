# difffuzz divergence, seed 37598
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: [117, 107, 107, 97, 92, 53, 51, 49, 49, 49, 48, 48, 48, 48, 44, 44, 44, 41, 40, 39, 39, 32, 32, 32]
#   mm:      [2560, 107, 107, 53, 51, 49, 49, 49, 48, 44, 44, 44, 41, 40, 39, 39, 32, 32, 32]
# signature: CPython: [#, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #] | mm: [#, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #, #]
import math
try:
    data = [ord(c) for c in (repr(chr(2560)) + str((1, 5, 1, 3, )).join(list({'k0': -1, 'k1': 100})))]
    print(sorted(data, key=None, reverse=True))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
