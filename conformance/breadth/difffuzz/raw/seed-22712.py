# difffuzz divergence, seed 22712
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 16: '\u0e3d' ("invalid literal for int() with base 16: '\\u0e3d'",)
#   mm:      caught ValueError invalid literal for int() with base 16: '฽' ("invalid literal for int() with base 16: '฽'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '\u#e#d' ("invalid literal for int() with b | mm: caught ValueError invalid literal for int() with base #: '฽' ("invalid literal for int() with base #
import math
try:
    try:
        print('3.5'[:].zfill(1).join(str([1, 0]).split(' ')).center(12, chr(42)) + int(chr(3645).lstrip(), 16))
    except ValueError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
