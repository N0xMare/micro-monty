# difffuzz divergence, seed 43856
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u09b4'
#   mm:      EXC ValueError could not convert string to float: '঴'
# signature: CPython: EXC ValueError could not convert string to float: '\u#b#' | mm: EXC ValueError could not convert string to float: '঴'
import math
try:
    def make22(start):
        total = start
        def step(by=1):
            return total
    s = make22(math.ceil(float(chr(2484))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
