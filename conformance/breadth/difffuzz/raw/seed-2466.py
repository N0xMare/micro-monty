# difffuzz divergence, seed 2466
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u243a'
#   mm:      EXC ValueError invalid literal for int() with base 0: '␺'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a' | mm: EXC ValueError invalid literal for int() with base #: '␺'
import math
try:
    v2 = 'a'
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n16 = 1
    while n16 > 0:
        print([(int(chr(9274).replace(v2.replace(v2, 'a,b,,c'), 'a,b,,c'), 0) + i17) for i17 in range(-2, 6) if i17 % 4], end='\U0001f600 ok')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
