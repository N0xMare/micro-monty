# difffuzz divergence, seed 38677
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0dc7--AbC'
#   mm:      EXC ValueError invalid literal for int() with base 10: '෇--AbC'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#dc#--AbC' | mm: EXC ValueError invalid literal for int() with base #: '෇--AbC'
import math
try:
    n11 = 2
    while n11 > 0:
        try:
            print(chr(3758) + n11)
        finally:
            print("finally", +int((chr(3527) + 'AbC'.rjust(5, chr(45)))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
