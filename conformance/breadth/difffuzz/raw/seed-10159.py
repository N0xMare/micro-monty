# difffuzz divergence, seed 10159
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v2 (('', -1180591620717411303424), ('a,b,,c', 1), ('\u085f', 3), 1, 3)
#   mm:      v2 (('', -1180591620717411303424), ('a,b,,c', 1), ('࡟', 3), 1, 3)
# signature: CPython: v# (('', -#), ('a,b,,c', #), ('\u#f', #), #, #) | mm: v# (('', -#), ('a,b,,c', #), ('࡟', #), #, #)
import math
try:
    v2 = (tuple(sorted({chr(2143): divmod(-3, -1)[0], ''.zfill(0): -(2 ** 70), 'a,b,,c'.zfill(2): (1 if False else 1)}.items())) + tuple(sorted(list(map(lambda q: q * 1 + 1, [0, 2])))))
    print('v2', v2)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
