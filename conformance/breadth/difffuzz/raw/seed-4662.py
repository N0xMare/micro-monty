# difffuzz divergence, seed 4662
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: one ('\u0b65', 0)
#   mm:      one ('୥', 0)
# signature: CPython: one ('\u#b#', #) | mm: one ('୥', #)
import math
try:
    match tuple(sorted({chr(2917): 'x\ny'.lower().rfind('x\ny'.zfill(3))}.items())):
        case (a, "x"):
            print("pair", a)
        case [x]:
            print("one", x)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
