# difffuzz divergence, seed 35635
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '0\u0ba5'
#   mm:      EXC ValueError could not convert string to float: '0஥'
# signature: CPython: EXC ValueError could not convert string to float: '#\u#ba#' | mm: EXC ValueError could not convert string to float: '#஥'
import math
try:
    try:
        print(float(chr(2981).zfill(2)))
    except ArithmeticError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
