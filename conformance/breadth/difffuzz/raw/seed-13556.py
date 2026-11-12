# difffuzz divergence, seed 13556
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: "\u17741{'k1': 0, 'k0': 3, 'k2': 255}3{'k1': 0, 'k0': 3, 'k2': 255}1"
#   mm:      EXC ValueError invalid literal for int() with base 16: "᝴1{'k1': 0, 'k0': 3, 'k2': 255}3{'k1': 0, 'k0': 3, 'k2': 255}1"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "\u#{'k#': #, 'k#': #, 'k#': #}#{'k#': #, 'k#' | mm: EXC ValueError invalid literal for int() with base #: "᝴#{'k#': #, 'k#': #, 'k#': #}#{'k#': #, 'k#':
import math
try:
    v9 = (chr(6004)[:-4:-1] + str(dict(sorted({'k0': 3, 'k1': 0, 'k2': 255}.items(), key=lambda kv: kv[1]))).join([str(q) for q in [len(w) for w in ['a', '123', '\u00df']]]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n22 = 6
    while n22 > 0:
            lst0 = [3, 1, 2]
            lst0.append(divmod(+((1 ** (i10 % 7)) if (False and False) else int(v9, 16)), {'k0': 3, 'k1': 0, 'k2': 7, 'k3': 1}['k0'])[0])
            try:
                print(float('%-6s|' % (1e-05,).title()))
            finally:
                print("finally", 3 ** 50)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
