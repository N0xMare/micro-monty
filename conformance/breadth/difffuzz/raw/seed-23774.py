# difffuzz divergence, seed 23774
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0c29'
#   mm:      EXC ValueError could not convert string to float: '఩'
# signature: CPython: EXC ValueError could not convert string to float: '\u#c#' | mm: EXC ValueError could not convert string to float: '఩'
import math
try:
    v12 = float(chr(3113))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
