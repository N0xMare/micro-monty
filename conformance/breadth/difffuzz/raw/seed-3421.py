# difffuzz divergence, seed 3421
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u17df'
#   mm:      EXC ValueError invalid literal for int() with base 0: '៟'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#df' | mm: EXC ValueError invalid literal for int() with base #: '៟'
import math
try:
    def make4(start):
        total = start
        def step(by=1):
            return total
    s = make4(int(chr(6111).removesuffix('{}-{!r}-{:>4}'.format(+-1, '\u00df', -5)), 0))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
