# difffuzz divergence, seed 39164
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0de0'
#   mm:      EXC ValueError could not convert string to float: '෠'
# signature: CPython: EXC ValueError could not convert string to float: '\u#de#' | mm: EXC ValueError could not convert string to float: '෠'
import math
try:
    def risky():
        try:
            print(float(chr(3552)))
        except ArithmeticError as e:
            print("caught", type(e).__name__, e, e.args)
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
