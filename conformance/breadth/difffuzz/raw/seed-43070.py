# difffuzz divergence, seed 43070
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 10: '\u0ca9' ("invalid literal for int() with base 10: '\\u0ca9'",)
#   mm:      caught ValueError invalid literal for int() with base 10: '಩' ("invalid literal for int() with base 10: '಩'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '\u#ca#' ("invalid literal for int() with b | mm: caught ValueError invalid literal for int() with base #: '಩' ("invalid literal for int() with base #
import math
try:
    try:
        print(int(chr(3241)))
    except ValueError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
