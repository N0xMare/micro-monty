# difffuzz divergence, seed 37352
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u1a9d'
#   mm:      EXC ValueError invalid literal for int() with base 16: '᪝'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#d' | mm: EXC ValueError invalid literal for int() with base #: '᪝'
import math
try:
    if (w := pow(divmod(-divmod(42, 1)[1], int(chr(6813), 16))[0], 11, 7)) > 23:
        print("walrus", w)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
