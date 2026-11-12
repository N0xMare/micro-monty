# difffuzz divergence, seed 18118
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 10: '*****\u2066' ("invalid literal for int() with base 10: '*****\\u2066'",)
#   mm:      caught ValueError invalid literal for int() with base 10: '*****⁦' ("invalid literal for int() with base 10: '*****⁦'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '*****\u#' ("invalid literal for int() with | mm: caught ValueError invalid literal for int() with base #: '*****⁦' ("invalid literal for int() with b
import math
try:
    def risky():
        try:
            print(int(chr(8294).rjust(6, chr(42))))
        except (TypeError, ValueError) as e:
            print("caught", type(e).__name__, e, e.args)
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
