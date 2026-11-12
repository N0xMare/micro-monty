# difffuzz divergence, seed 20906
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2b75'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⭵'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '⭵'
import math
try:
    v15 = ((int(chr(11125)) / 'tab\there'.title().find('\U0001f600 ok'.replace('', 'AbC'))) ** 3)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
