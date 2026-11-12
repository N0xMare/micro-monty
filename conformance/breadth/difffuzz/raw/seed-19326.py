# difffuzz divergence, seed 19326
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: '\u128f'
#   mm:      '኏'
# signature: CPython: '\u#f' | mm: '኏'
import math
try:
    print(repr(chr(4751)).split()[0], sep='\U0001f600 ok')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
