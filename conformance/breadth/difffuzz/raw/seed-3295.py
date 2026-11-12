# difffuzz divergence, seed 3295
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ["0-'\\u0ee9'- -20"]caféother
#   mm:      ["0-'໩'- -20"]caféother
# signature: CPython: ["#-'\\u#ee#'- -#"]caféother | mm: ["#-'໩'- -#"]caféother
import math
try:
    lst0 = [3, 1, 2]
    lst0[-2] = divmod(255, -17)[0]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print('{}-{!r}-{:>4}'.format(pow((-1 if False else 255), 8, -5), chr(3817).strip(''.partition('s')[0]), sum((lst0 + lst0))).splitlines(), end='caf\u00e9')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    match {}:
        case [x, y, *rest]:
            print("many", x, y, rest)
        case _:
            print("other")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
