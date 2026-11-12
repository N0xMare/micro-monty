# difffuzz divergence, seed 21893
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0dc7'
#   mm:      EXC ValueError could not convert string to float: '෇'
# signature: CPython: EXC ValueError could not convert string to float: '\u#dc#' | mm: EXC ValueError could not convert string to float: '෇'
import math
try:
    v2 = isinstance(math.isqrt(float(chr(3527))), bool)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
