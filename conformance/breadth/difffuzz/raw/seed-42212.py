# difffuzz divergence, seed 42212
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0e7d'
#   mm:      EXC ValueError could not convert string to float: '๽'
# signature: CPython: EXC ValueError could not convert string to float: '\u#e#d' | mm: EXC ValueError could not convert string to float: '๽'
import math
try:
    print(float(chr(3709).expandtabs()), sep='', end='a,b,,c')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
