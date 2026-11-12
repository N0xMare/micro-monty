# difffuzz divergence, seed 10110
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u1737'
#   mm:      EXC ValueError invalid literal for int() with base 16: '᜷'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '᜷'
import math
try:
    print((-17 < divmod(dict(sorted({'k0': 255, 'k1': 0, 'k2': -3}.items(), key=lambda kv: kv[1]))['k1'], pow(int(-2.25), 25, 1000))[1] <= int(chr(5943), 16)), end='hello')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
