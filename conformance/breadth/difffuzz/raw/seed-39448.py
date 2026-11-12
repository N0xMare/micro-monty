# difffuzz divergence, seed 39448
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 0: '\u0e67' ("invalid literal for int() with base 0: '\\u0e67'",)
#   mm:      caught ValueError invalid literal for int() with base 0: '๧' ("invalid literal for int() with base 0: '๧'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '\u#e#' ("invalid literal for int() with ba | mm: caught ValueError invalid literal for int() with base #: '๧' ("invalid literal for int() with base #
import math
try:
    try:
        print(([len(w) for w in [str(q) for q in list(range(0, 1, 3))]] * 0) < '-42'.swapcase()[int(chr(3687), 0)])
    except (TypeError, ValueError) as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
