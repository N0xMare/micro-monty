# difffuzz divergence, seed 33741
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 10: '\u197b' ("invalid literal for int() with base 10: '\\u197b'",)
#   mm:      caught ValueError invalid literal for int() with base 10: '᥻' ("invalid literal for int() with base 10: '᥻'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '\u#b' ("invalid literal for int() with bas | mm: caught ValueError invalid literal for int() with base #: '᥻' ("invalid literal for int() with base #
import math
try:
    def risky():
        try:
            print(int(chr(6523)[-3:]))
        except ValueError as e:
            print("caught", type(e).__name__, e, e.args)
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
