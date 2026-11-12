# difffuzz divergence, seed 31692
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0a50'
#   mm:      EXC ValueError invalid literal for int() with base 10: '੐'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#' | mm: EXC ValueError invalid literal for int() with base #: '੐'
import math
try:
    print(int((chr(2640) + repr([]).join(list({})))), sep='caf\u00e9', end='Hello World')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
