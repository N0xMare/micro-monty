# difffuzz divergence, seed 40575
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0ea6'
#   mm:      EXC ValueError could not convert string to float: '຦'
# signature: CPython: EXC ValueError could not convert string to float: '\u#ea#' | mm: EXC ValueError could not convert string to float: '຦'
import math
try:
    v5 = chr(3750)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i6 in range(2):
        if i6 == 2:
            continue
        def make7(start):
            total = start
            def step(by=1):
                return total
        s = make7(round(round(-float(v5), 2)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
