# difffuzz divergence, seed 37507
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0fdb'
#   mm:      EXC ValueError invalid literal for int() with base 10: '࿛'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fdb' | mm: EXC ValueError invalid literal for int() with base #: '࿛'
import math
try:
    print(divmod(int(chr(4059).replace('tab\there'.ljust(9, chr(45)), 'Hello World'.expandtabs())), ((2 if isinstance([10, 3, 1], (int, str)) else divmod(-1, -17)[0]) if ((2 in v4) and isinstance(1000, list)) else (math.ceil(-2.25) ^ (1 if True else 255)))), sep='tab\there')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
