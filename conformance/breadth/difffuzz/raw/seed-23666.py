# difffuzz divergence, seed 23666
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2fef'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⿯'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fef' | mm: EXC ValueError invalid literal for int() with base #: '⿯'
import math
try:
    for i4 in range(-1, 8):
        while n6 > 0:
            print(int('Hello World', 2))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    (a, b), c = tuple(sorted({k29: len(k29) for k29 in list('{}-{!r}-{:>4}'.format(i4, '3.5', 255))}.items())), ~(int(chr(12271)) if (False or bool('caf\u00e9')) else '-42'.replace('123', '  pad  ').rfind('{}-{!r}-{:>4}'.format(7, 'a', n6)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
