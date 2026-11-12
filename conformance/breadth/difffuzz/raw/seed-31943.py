# difffuzz divergence, seed 31943
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0a65'
#   mm:      EXC ValueError could not convert string to float: '੥'
# signature: CPython: EXC ValueError could not convert string to float: '\u#a#' | mm: EXC ValueError could not convert string to float: '੥'
import math
try:
    v12 = float(chr(2661))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
