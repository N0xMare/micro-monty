# difffuzz divergence, seed 31479
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: 'Hello World\u2d26'
#   mm:      EXC ValueError invalid literal for int() with base 10: 'Hello World⴦'
# signature: CPython: EXC ValueError invalid literal for int() with base #: 'Hello World\u#d#' | mm: EXC ValueError invalid literal for int() with base #: 'Hello World⴦'
import math
try:
    def make5(start):
        total = start
        def step(by=1):
            return total
    s = make5(int(('Hello World'.replace('AbC', '\u00df').partition('s')[0] + chr(11558))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
