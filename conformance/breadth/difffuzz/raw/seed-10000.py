# difffuzz divergence, seed 10000
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught KeyError '\u0e3b' ('\u0e3b',)
#   mm:      caught KeyError '฻' ('฻',)
# signature: CPython: caught KeyError '\u#e#b' ('\u#e#b',) | mm: caught KeyError '฻' ('฻',)
import math
try:
    try:
        print({}[chr(3643)])
    except LookupError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
