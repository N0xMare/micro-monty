# difffuzz divergence, seed 5064
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u1754'
#   mm:      EXC ValueError could not convert string to float: '᝔'
# signature: CPython: EXC ValueError could not convert string to float: '\u#' | mm: EXC ValueError could not convert string to float: '᝔'
import math
try:
    v12 = float(chr(5972))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
