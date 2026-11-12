# difffuzz divergence, seed 7968
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u2b96'
#   mm:      EXC ValueError invalid literal for int() with base 36: '⮖'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '⮖'
import math
try:
    def make19(start):
        total = start
        def step(by=1):
            return total
    s = make19(int(chr(11158).lstrip('3.5'.expandtabs()[:4:-1]), 36))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
