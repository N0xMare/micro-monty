# difffuzz divergence, seed 39188
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '\u2d2c'
#   mm:      EXC ValueError invalid literal for int() with base 2: '⴬'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#c' | mm: EXC ValueError invalid literal for int() with base #: '⴬'
import math
try:
    v2 = '{}-{!r}-{:>4}'.format(int(chr(11564), 2), str({'k0': 3}).removeprefix('a'), list({'k0': 5, 'k1': 42, 'k2': 0}.values())[len((7, ))]).zfill(9)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
