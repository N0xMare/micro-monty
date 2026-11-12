# difffuzz divergence, seed 18987
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u137d'
#   mm:      EXC ValueError invalid literal for int() with base 10: '፽'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d' | mm: EXC ValueError invalid literal for int() with base #: '፽'
import math
try:
    n4 = 3
    while n4 > 0:
        print(int(chr(4989), 10))
        try:
            print(round((pow({}['zz'], 37, -5) / 10 ** 30 + 7)) // (abs(((n4 if True else n4) ** (len({'k0': 10, 'k1': 1}) % 7))).bit_length() >> ((int(1e-05) if all(list(filter(lambda q: q > -2, lst0))) else pow(max([1]), 28, -5)) % 9)))
        except LookupError as e:
            print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
