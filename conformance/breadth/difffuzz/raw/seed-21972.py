# difffuzz divergence, seed 21972
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1756'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᝖'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '᝖'
import math
try:
    v10 = '{}-{!r}-{:>4}'.format(sum([len(w) for w in '3.5'.splitlines()]), '\u00df', math.trunc(abs(int(chr(5974), 10)) % 30))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
