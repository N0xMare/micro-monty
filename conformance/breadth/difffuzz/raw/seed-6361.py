# difffuzz divergence, seed 6361
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 2: '\u13fe' ("invalid literal for int() with base 2: '\\u13fe'",)
#   mm:      caught ValueError invalid literal for int() with base 2: '᏾' ("invalid literal for int() with base 2: '᏾'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '\u#fe' ("invalid literal for int() with ba | mm: caught ValueError invalid literal for int() with base #: '᏾' ("invalid literal for int() with base #
import math
try:
    def risky():
        try:
            print(list({}.values())[+int(chr(5118), 2)])
        except ValueError as e:
            print("caught", type(e).__name__, e, e.args)
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
