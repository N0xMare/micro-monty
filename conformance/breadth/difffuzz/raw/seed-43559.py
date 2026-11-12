# difffuzz divergence, seed 43559
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v24 '\u0ade'
#   mm:      v24 '૞'
# signature: CPython: v# '\u#ade' | mm: v# '૞'
import math
try:
    v24 = repr(chr(2782))
    print('v24', v24)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
