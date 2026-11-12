# difffuzz divergence, seed 41752
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u2d6e'
#   mm:      EXC ValueError invalid literal for int() with base 0: '⵮'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#e' | mm: EXC ValueError invalid literal for int() with base #: '⵮'
import math
try:
    if any(list(reversed(sorted({1000})))[5::1]):
        match None:
            case int(n) if n > 10:
                print("big", n)
    else:
        for i9 in range(3):
            def f10(p0, p1):
                return repr(('AbC', i9, ))[divmod(f4(5, [2], 'a'), (i9 ** (i9 % 7)))[1]].swapcase()
            match '%05d' % (f4(int('tab\there'[2]), [ord(c) for c in f'{1.5:.4g} '], ('AbC' + '-42').title()), math.trunc(abs(f4(abs(2).bit_length(), [(i9 + i15) for i15 in range(3) if i15 % 4], ('a' * (1000 % 4)))) % 30),):
                case int(n) if n > 10:
                    print("big", n)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C29:
        def __init__(self, v, w=0):
            self.w = w
        def __repr__(self):
            return f'C29({self.v!r}, {self.w})'
    objs = [C29(list(map(lambda q: q * 2 + 1, ((lst0 * 3) + list(map(lambda q: q * 0 + 1, [1, 1, 100])))))[(pow((1000 << (i9 % 70)), 1, 1000) << (math.trunc(2.5) % 70))], -4), C29(int(chr(11630).swapcase(), 0), -3), C29(len(()), -4), C29.make(2)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
