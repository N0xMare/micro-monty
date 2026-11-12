# difffuzz divergence, seed 21869
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0b49'
#   mm:      EXC ValueError invalid literal for int() with base 10: '୉'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '୉'
import math
try:
    if ('x\ny' in sorted('{}-{!r}-{:>4}'.format(10, 'mississippi', 2).split(), key=len)):
        class Err3(Exception):
                self.code = code
        try:
            try:
                raise Err3('-42'.center(9, chr(42)).swapcase().ljust(11, chr(45)).expandtabs(), 8)
            except Err3 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
    else:
        def f4(p0, p1, *rest, kw=3):
            return int('{}-{!r}-{:>4}'.format(math.isqrt((2.5 ** 2)), 'a'.partition('s')[0].removeprefix('\U0001f600 ok'[0::-1]), (len((-3, -17, )) | abs(p1).bit_length())))
        print('f4(list(range(-1, 8, -1)), int(chr(2889)))', f4(list(range(-1, 8, -1)), int(chr(2889))))
        class Ctx5:
            def __init__(self, name):
                self.name = name
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
