# difffuzz divergence, seed 37893
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError could not convert string to float: '\u0fed' ("could not convert string to float: '\\u0fed'",)
#   mm:      caught ValueError could not convert string to float: '࿭' ("could not convert string to float: '࿭'",)
# signature: CPython: caught ValueError could not convert string to float: '\u#fed' ("could not convert string to float: ' | mm: caught ValueError could not convert string to float: '࿭' ("could not convert string to float: '࿭'",)
import math
try:
    n21 = 6
    while n21 > 0:
        n21 -= 1
        try:
            print(float(chr(4077).capitalize()))
        except Exception as e:
            print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
