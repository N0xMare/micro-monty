# difffuzz divergence, seed 41599
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0cfc'
#   mm:      EXC ValueError invalid literal for int() with base 10: '೼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cfc' | mm: EXC ValueError invalid literal for int() with base #: '೼'
import math
try:
    print({**{}, "z": 1}, [*divmod((int(chr(3324)) >> (10 ** 30 + 7 % 9)), max([i + v for i, v in enumerate([len(w) for w in ['tab\there', 'caf\u00e9', 'a,b,,c']], 2)])), *(list(range(5, 8, 1)) * 2)])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
