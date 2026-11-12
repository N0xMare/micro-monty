# difffuzz divergence, seed 43074
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '00000\u0557'
#   mm:      EXC ValueError invalid literal for int() with base 10: '00000՗'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#' | mm: EXC ValueError invalid literal for int() with base #: '#՗'
import math
try:
    try:
        print(len((('\u00df', '3.5', ) + ((0, 0, 1000, 0, ) + (10, )))[-3:]) ** -3)
    except LookupError as e:
        print("caught", type(e).__name__, e, e.args)
    finally:
        print("finally", int(chr(1367).zfill(6).zfill(3)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
