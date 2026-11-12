# difffuzz divergence, seed 12548
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u245a'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⑚'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a' | mm: EXC ValueError invalid literal for int() with base #: '⑚'
import math
try:
    print(divmod(sum([len(w) for w in 'hello'.splitlines()]), int(chr(9306).zfill(1))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
