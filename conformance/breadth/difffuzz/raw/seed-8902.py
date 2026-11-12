# difffuzz divergence, seed 8902
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 16: '\u05f9' ("invalid literal for int() with base 16: '\\u05f9'",)
#   mm:      caught ValueError invalid literal for int() with base 16: '׹' ("invalid literal for int() with base 16: '׹'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '\u#f#' ("invalid literal for int() with ba | mm: caught ValueError invalid literal for int() with base #: '׹' ("invalid literal for int() with base #
import math
try:
    try:
        print(list({k31: len(k31) for k31 in list('\U0001f600 ok'.zfill(6))}.values())[int(chr(1529), 16)])
    except Exception as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
