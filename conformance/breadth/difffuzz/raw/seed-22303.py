# difffuzz divergence, seed 22303
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0b51'
#   mm:      EXC ValueError invalid literal for int() with base 10: '୑'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '୑'
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
        class C22:
            def __init__(self, v, w=0):
                self.w = w
        objs = [C22(int(chr(2897)), 3), C22(len({255}), 4), C22(('%d' % (pow(i8, 16, 2 ** 61 - 1),) * (len(list(filter(lambda q: q > -1, lst0))) % 4)).count('hello'.join(sorted(['Hello World', '-42', '\U0001f600 ok'], key=len)).lstrip(f'{1e300:08.2f} ')), -3), C22.make(2)]
        for i23 in range(-2, 7):
                print("walrus", w)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
