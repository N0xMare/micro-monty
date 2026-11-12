# difffuzz divergence, seed 29376
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u2e6b'
#   mm:      EXC ValueError invalid literal for int() with base 0: '⹫'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#b' | mm: EXC ValueError invalid literal for int() with base #: '⹫'
import math
try:
    try:
        print((round(((v15 % -0.0) / (42 / -3))) if False else round(0.1)) // int(chr(11883), 0))
    except ZeroDivisionError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
