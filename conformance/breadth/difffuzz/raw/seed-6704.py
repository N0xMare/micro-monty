# difffuzz divergence, seed 6704
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0e3e'
#   mm:      EXC ValueError invalid literal for int() with base 16: '฾'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#e' | mm: EXC ValueError invalid literal for int() with base #: '฾'
import math
try:
    try:
        print(-3 ** -2)
    finally:
        print("finally", int(chr(3646).title(), 16))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
