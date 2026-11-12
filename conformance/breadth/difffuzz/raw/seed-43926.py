# difffuzz divergence, seed 43926
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u2430'
#   mm:      EXC ValueError could not convert string to float: '␰'
# signature: CPython: EXC ValueError could not convert string to float: '\u#' | mm: EXC ValueError could not convert string to float: '␰'
import math
try:
    v16 = float(chr(9264).swapcase())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
