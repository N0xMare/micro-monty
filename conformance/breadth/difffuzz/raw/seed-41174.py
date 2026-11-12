# difffuzz divergence, seed 41174
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError could not convert string to float: '\u0eec' ("could not convert string to float: '\\u0eec'",)
#   mm:      caught ValueError could not convert string to float: '໬' ("could not convert string to float: '໬'",)
# signature: CPython: caught ValueError could not convert string to float: '\u#eec' ("could not convert string to float: ' | mm: caught ValueError could not convert string to float: '໬' ("could not convert string to float: '໬'",)
import math
try:
    try:
        print(float(chr(3820)))
    except Exception as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
