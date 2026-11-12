# difffuzz divergence, seed 1471
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u2d76'
#   mm:      EXC ValueError invalid literal for int() with base 16: '⵶'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#' | mm: EXC ValueError invalid literal for int() with base #: '⵶'
import math
try:
    lst0 = [3, 1, 2]
    lst0.append((int(chr(11638), 16) << (len(sorted({10, 5})) % 70)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
