# difffuzz divergence, seed 1569
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0b8b'
#   mm:      EXC ValueError could not convert string to float: '஋'
# signature: CPython: EXC ValueError could not convert string to float: '\u#b#b' | mm: EXC ValueError could not convert string to float: '஋'
import math
try:
    print(float(chr(2955)), end='a')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
