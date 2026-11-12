# difffuzz divergence, seed 25920
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 10: 'tab\there\u2e67AbC\u2e673.5\u2e67123' ("invalid literal for int() with base 10: 'tab\\there\\u2e67AbC\\u2e673.5\\u2e67123'",)
#   mm:      caught ValueError invalid literal for int() with base 10: 'tab\there⹧AbC⹧3.5⹧123' ("invalid literal for int() with base 10: 'tab\\there⹧AbC⹧3.5⹧123'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: 'tab\there\u#e#AbC\u#e#.#\u#e#' ("invalid l | mm: caught ValueError invalid literal for int() with base #: 'tab\there⹧AbC⹧#.#⹧#' ("invalid literal for
import math
try:
    try:
        print(int(chr(11879).join(['tab\there', 'AbC', '3.5', '123']).split(' ')[0]))
    except ValueError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
