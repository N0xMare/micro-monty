# difffuzz divergence, seed 9807
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0de5'
#   mm:      EXC ValueError invalid literal for int() with base 10: '෥'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#de#' | mm: EXC ValueError invalid literal for int() with base #: '෥'
import math
try:
    lst0 = [3, 1, 2]
    lst0.extend(list({f'{None!r}|{lst0} ': int(chr(3557)), ('3.5' * (abs(7).bit_length() % 4)): int(math.tau), '{}-{!r}-{:>4}'.format(round(1e-05), ('3.5' + '-42'), (1 & -17)): {'hello': 1000, '  pad  ': 3}['k0']}.values()))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
