# difffuzz divergence, seed 27213
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1719'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᜙'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '᜙'
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(divmod(~divmod(sum(lst0), int(2.5))[0], (int(chr(5913), 10) << (len({'k0': 10}) % 70))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
