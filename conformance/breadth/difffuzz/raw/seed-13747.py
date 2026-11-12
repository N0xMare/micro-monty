# difffuzz divergence, seed 13747
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0cd8'
#   mm:      EXC ValueError could not convert string to float: '೘'
# signature: CPython: EXC ValueError could not convert string to float: '\u#cd#' | mm: EXC ValueError could not convert string to float: '೘'
import math
try:
    def make1(start):
        total = start
        def step(by=1):
            return total
    s = make1(int(float(chr(3288))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
