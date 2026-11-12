# difffuzz divergence, seed 24945
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u20fb'
#   mm:      EXC ValueError could not convert string to float: '⃻'
# signature: CPython: EXC ValueError could not convert string to float: '\u#fb' | mm: EXC ValueError could not convert string to float: '⃻'
import math
try:
    try:
        print(float(chr(8443)))
    except KeyError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
