# difffuzz divergence, seed 23438
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u206f'
#   mm:      EXC ValueError invalid literal for int() with base 36: '⁯'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f' | mm: EXC ValueError invalid literal for int() with base #: '⁯'
import math
try:
    if (w := int(chr(8303)[:2], 36)) > -3:
        print("walrus", w)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
