# difffuzz divergence, seed 22773
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u2d73'
#   mm:      EXC ValueError invalid literal for int() with base 0: '⵳'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#' | mm: EXC ValueError invalid literal for int() with base #: '⵳'
import math
try:
    lst0 = [3, 1, 2]
    lst0.insert(3, (len('123'.replace('a', '').split(',')) ** (int(chr(11635).capitalize(), 0) % 7)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
