# difffuzz divergence, seed 5901
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError ⹨ ('\u2e68',)
#   mm:      caught ValueError ⹨ ('⹨',)
# signature: CPython: caught ValueError ⹨ ('\u#e#',) | mm: caught ValueError ⹨ ('⹨',)
import math
try:
    try:
        raise ValueError(chr(11880))
    except ValueError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
