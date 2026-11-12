# difffuzz divergence, seed 10213
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2430'
#   mm:      EXC ValueError invalid literal for int() with base 10: '␰'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '␰'
import math
try:
    class C11:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C11(int(chr(9264)), -3), C11(int('%d' % (repr(['tab\there']), ('a,b,,c' * (10 % 4)),).removesuffix(('a,b,,c' * (n3 % 4)).center(6, chr(42)))), 1), C11(int('mississippi'[1:-3:2].zfill(5).join('{}-{!r}-{:>4}'.format(n3, '', n3).splitlines())), 0), C11.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
