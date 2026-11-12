# difffuzz divergence, seed 26701
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u12bf'
#   mm:      EXC ValueError invalid literal for int() with base 10: '኿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#bf' | mm: EXC ValueError invalid literal for int() with base #: '኿'
import math
try:
    try:
        print(undefined_name)
    finally:
        print("finally", int(chr(4799), 10))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
