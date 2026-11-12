# difffuzz divergence, seed 21787
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u1249'
#   mm:      EXC ValueError could not convert string to float: '቉'
# signature: CPython: EXC ValueError could not convert string to float: '\u#' | mm: EXC ValueError could not convert string to float: '቉'
import math
try:
    match int((0.1 / float(chr(4681)))):
        case list(l):
            print("list", len(l))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
