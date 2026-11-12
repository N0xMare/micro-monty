# difffuzz divergence, seed 11969
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0381'
#   mm:      EXC ValueError invalid literal for int() with base 10: '΁'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '΁'
import math
try:
    lst0[0] = (pow(int(chr(897)), 9, 2 ** 61 - 1) >> (len(chr(11860).splitlines()) % 9))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
