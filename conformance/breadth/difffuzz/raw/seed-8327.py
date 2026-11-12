# difffuzz divergence, seed 8327
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: 'k00000000\u0f6ek10000000\u0f6ek20000000\u0f6ek3'
#   mm:      EXC ValueError invalid literal for int() with base 10: 'k00000000཮k10000000཮k20000000཮k3'
# signature: CPython: EXC ValueError invalid literal for int() with base #: 'k#\u#f#ek#\u#f#ek#\u#f#ek#' | mm: EXC ValueError invalid literal for int() with base #: 'k#཮k#཮k#཮k#'
import math
try:
    v6 = ''
    v6 += chr(3950).zfill(8).join(list({'k0': 2, 'k1': -17, 'k2': 5, 'k3': 10})).strip(v6)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    match ((f'{v6!r:>12} and ' * (-3 % 4)), '{}-{!r}-{:>4}'.format(int(v6), (v6 + 'mississippi'), min([-17, 42, 1, 5, 42])).lower(), ):
        case {}:
            print("any mapping")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
