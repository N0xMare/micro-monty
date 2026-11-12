# difffuzz divergence, seed 17085
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v2 ["'\\u2b75'"]
#   mm:      v2 ["'⭵'"]
# signature: CPython: v# ["'\\u#b#'"] | mm: v# ["'⭵'"]
import math
try:
    v2 = repr(chr(11125)).split()
    print('v2', v2)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
