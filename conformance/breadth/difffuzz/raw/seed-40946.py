# difffuzz divergence, seed 40946
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: [0]ßEXC ValueError invalid literal for int() with base 0: '\u245f'
#   mm:      [0]ßEXC ValueError invalid literal for int() with base 0: '⑟'
# signature: CPython: [#]ßEXC ValueError invalid literal for int() with base #: '\u#f' | mm: [#]ßEXC ValueError invalid literal for int() with base #: '⑟'
import math
try:
    print(list(map(lambda q: q * -1 + 1, [1])), end='\u00df')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def make20(start):
        total = start
        def step(by=1):
            return total
    s = make20(int(chr(9311), 0))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
