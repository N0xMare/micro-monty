# difffuzz divergence, seed 13005
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0fee'
#   mm:      EXC ValueError invalid literal for int() with base 10: '࿮'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fee' | mm: EXC ValueError invalid literal for int() with base #: '࿮'
import math
try:
    def f2(p0, p1):
        class Ctx3:
            def __init__(self, name):
                self.name = name
        try:
            print('{}-{!r}-{:>4}'.format(int('\u00df', 2), '%%%d' % (-2.25, 'Hello World',), math.isqrt(3.0)).expandtabs().removeprefix('tab\there'[p0].center(11, chr(45)).join(sorted('tab\there'.split(), key=lambda w: (-len(w), w)))).index('\u00df'))
        except ValueError as e:
            print("caught", type(e).__name__, e, e.args)
    print("f2((len(f'{3:b}') >> (pow(sum(sorted([1000, 42, 10, 2], reverse=True)), 12, 1000) % 9)), (int(chr(4078)) * pow(divmod(-(10 ** 25), (100 * 5))[1], 20, 1000)))", f2((len(f'{3:b}') >> (pow(sum(sorted([1000, 42, 10, 2], reverse=True)), 12, 1000) % 9)), (int(chr(4078)) * pow(divmod(-(10 ** 25), (100 * 5))[1], 20, 1000))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
