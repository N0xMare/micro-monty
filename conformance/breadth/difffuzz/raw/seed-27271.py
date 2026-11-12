# difffuzz divergence, seed 27271
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u0fed'
#   mm:      EXC ValueError invalid literal for int() with base 0: '࿭'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fed' | mm: EXC ValueError invalid literal for int() with base #: '࿭'
import math
try:
    try:
        print(float('\u00df'))
    finally:
        print("finally", int('a,b,,c'[:3].strip().join(chr(4077).split(',')), 0))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
