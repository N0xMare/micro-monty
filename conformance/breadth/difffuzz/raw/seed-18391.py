# difffuzz divergence, seed 18391
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError ௑ ('\u0bd1',)
#   mm:      caught ValueError ௑ ('௑',)
# signature: CPython: caught ValueError ௑ ('\u#bd#',) | mm: caught ValueError ௑ ('௑',)
import math
try:
    n38 = 4
    while n38 > 0:
        try:
            raise ValueError(chr(3025).zfill(0).swapcase())
        except (TypeError, ValueError) as e:
            print("caught", type(e).__name__, e, e.args)
            print("finally", sorted({s40 % 4 for s40 in lst0})[math.factorial(-7.125)])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
