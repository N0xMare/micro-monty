# difffuzz divergence, seed 2600
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 10: '\u05ed' ("invalid literal for int() with base 10: '\\u05ed'",)
#   mm:      caught ValueError invalid literal for int() with base 10: '׭' ("invalid literal for int() with base 10: '׭'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '\u#ed' ("invalid literal for int() with ba | mm: caught ValueError invalid literal for int() with base #: '׭' ("invalid literal for int() with base #
import math
try:
    try:
        print(int(chr(1517)))
    except Exception as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
