# difffuzz divergence, seed 2402
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError could not convert string to float: '\u0df0' ("could not convert string to float: '\\u0df0'",)
#   mm:      caught ValueError could not convert string to float: '෰' ("could not convert string to float: '෰'",)
# signature: CPython: caught ValueError could not convert string to float: '\u#df#' ("could not convert string to float: ' | mm: caught ValueError could not convert string to float: '෰' ("could not convert string to float: '෰'",)
import math
try:
    try:
        print(float(chr(3568)))
    except (TypeError, ValueError) as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
