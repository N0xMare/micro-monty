# difffuzz divergence, seed 34623
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0b7f'
#   mm:      EXC ValueError could not convert string to float: '୿'
# signature: CPython: EXC ValueError could not convert string to float: '\u#b#f' | mm: EXC ValueError could not convert string to float: '୿'
import math
try:
    print(abs(float(chr(2943))), end='AbC')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
