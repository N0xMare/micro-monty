# difffuzz divergence, seed 34517
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0d99'
#   mm:      EXC ValueError invalid literal for int() with base 10: '඙'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#' | mm: EXC ValueError invalid literal for int() with base #: '඙'
import math
try:
    match int(chr(3481).removeprefix(f'{[3, 255]!r}{None!r}|')):
        case [x, y, *rest]:
            print("many", x, y, rest)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
