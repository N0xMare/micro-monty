# difffuzz divergence, seed 6432
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 2: '\u0e6f' ("invalid literal for int() with base 2: '\\u0e6f'",)
#   mm:      caught ValueError invalid literal for int() with base 2: '๯' ("invalid literal for int() with base 2: '๯'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '\u#e#f' ("invalid literal for int() with b | mm: caught ValueError invalid literal for int() with base #: '๯' ("invalid literal for int() with base #
import math
try:
    def risky():
        try:
            raise KeyError(int(chr(3695), 2))
        except Exception as e:
            print("caught", type(e).__name__, e, e.args)
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
