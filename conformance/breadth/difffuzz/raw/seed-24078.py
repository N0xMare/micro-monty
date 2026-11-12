# difffuzz divergence, seed 24078
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0de2'
#   mm:      EXC ValueError invalid literal for int() with base 16: '෢'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#de#' | mm: EXC ValueError invalid literal for int() with base #: '෢'
import math
try:
    if ((str(int(7.125)) in '-42'.zfill(6).swapcase()) and (2 ** 64 < (3 ** 50 ** (round(3.0) % 7)) <= ((1000 & 7) - (10 >> (7 % 9))))):
        def risky():
            try:
                print(int(repr(sorted('123'.split()[1].splitlines(), key=str.lower))))
            except Exception as e:
                print("caught", type(e).__name__, e, e.args)
    elif any([len(w) for w in sorted(list({'k0': 2}), key=lambda w: (-len(w), w))]):
        lst0 = [3, 1, 2]
        class C9:
            def __init__(self, v, w=0):
                self.w = w
            def __repr__(self):
                return f'C9({self.v!r}, {self.w})'
        objs = [C9((len((10, -1, '\U0001f600 ok', )[4:4:-1]) << ((pow({'k0': 255, 'k1': -17, 'k2': 2}['k0'], 6, -5) // 2 ** 64) % 70)), 4), C9(int(chr(3554), 16), 2), C9(([i + v for i, v in enumerate(sorted([2, 3], key=str), 0)][int(''.swapcase(), 0)] << (len((divmod(2, 100) + (() + (2, -1, )))) % 70)), 3), C9.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
