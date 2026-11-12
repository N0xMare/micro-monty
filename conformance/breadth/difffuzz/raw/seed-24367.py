# difffuzz divergence, seed 24367
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u125f'
#   mm:      EXC ValueError invalid literal for int() with base 36: '቟'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f' | mm: EXC ValueError invalid literal for int() with base #: '቟'
import math
try:
    print((len(list({'k0': 255, 'k1': 1, 'k2': 5}.values())) < int(chr(4703), 36) <= round(1.5)), sep='Hello World')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
