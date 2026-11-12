# difffuzz divergence, seed 11200
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0bda'
#   mm:      EXC ValueError could not convert string to float: '௚'
# signature: CPython: EXC ValueError could not convert string to float: '\u#bda' | mm: EXC ValueError could not convert string to float: '௚'
import math
try:
    print(float(chr(3034)), end='3.5')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
