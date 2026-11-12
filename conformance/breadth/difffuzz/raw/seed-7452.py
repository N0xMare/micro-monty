# difffuzz divergence, seed 7452
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u187b']
#   mm:      ['᡻']
# signature: CPython: ['\u#b'] | mm: ['᡻']
import math
try:
    data = chr(6267).replace(''.ljust(9, chr(42)), 'a,b,,c'.join(['AbC', '3.5', '  pad  ', '  pad  ']).swapcase()).split(',')
    print(sorted(data, key=len, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
