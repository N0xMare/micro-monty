# difffuzz divergence, seed 21710
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u181d'
#   mm:      EXC ValueError invalid literal for int() with base 16: '᠝'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d' | mm: EXC ValueError invalid literal for int() with base #: '᠝'
import math
try:
    match int(chr(6173), 16):
        case [x]:
            print("one", x)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
