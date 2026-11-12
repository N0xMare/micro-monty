# difffuzz divergence, seed 37846
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 10: "3-'3.5'-\u19dc0" ('invalid literal for int() with base 10: "3-\'3.5\'-\\u19dc0"',)
#   mm:      caught ValueError invalid literal for int() with base 10: "3-'3.5'-᧜0" ('invalid literal for int() with base 10: "3-\'3.5\'-᧜0"',)
# signature: CPython: caught ValueError invalid literal for int() with base #: "#-'#.#'-\u#dc#" ('invalid literal for int( | mm: caught ValueError invalid literal for int() with base #: "#-'#.#'-᧜#" ('invalid literal for int() wi
import math
try:
    n1 = 4
    while n1 > 0:
        n1 -= 1
        for i2 in range(8):
            try:
                print(int(chr(6620).join('{}-{!r}-{:>4}'.format(n1, '3.5', i2).replace('\u00df'.upper(), chr(7081)).split())))
            except ValueError as e:
                print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
