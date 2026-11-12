# difffuzz divergence, seed 30237
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ('\u0ad8',)
#   mm:      ('૘',)
# signature: CPython: ('\u#ad#',) | mm: ('૘',)
import math
try:
    print((chr(2776), ))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
