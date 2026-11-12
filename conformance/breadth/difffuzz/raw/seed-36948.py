# difffuzz divergence, seed 36948
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ('a', '\u0b29')
#   mm:      ('a', '଩')
# signature: CPython: ('a', '\u#b#') | mm: ('a', '଩')
import math
try:
    print(('a', chr(2857), ))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
