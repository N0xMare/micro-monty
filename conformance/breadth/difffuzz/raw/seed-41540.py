# difffuzz divergence, seed 41540
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err12 '\u0bd9' 1 wrapped
#   mm:      Err12 '௙' 1 wrapped
# signature: CPython: Err# '\u#bd#' # wrapped | mm: Err# '௙' # wrapped
import math
try:
    if (f'{None!a} and {1e300:e} {2:n}' + str(-1).partition(',')[1]).endswith('{}-{!r}-{:>4}'.format((3 << (100 % 70)), '{}-{!r}-{:>4}'.format(255, 'AbC', 255), 2 ** 64).join(sorted(['  pad  ', 'a', 'caf\u00e9', '\u00df'], key=len))):
        if (('3.5'[-2:-4:-2] * ([-3][7] % 4)).isspace() and ((abs(3).bit_length() if ([] == [3]) else {'k0': -1, 'k1': 7, 'k2': 100}['k1']) in ({2, -1, 10, 0, 1000} | frozenset([42, 10]) | {5}))):
            def gen5(n):
                    if k % 2 == 0:
                        yield k * -3
            for i6 in range(0):
                print(lst0)
            def f8(p0):
                return (int(('3.5'.swapcase() + '{}-{!r}-{:>4}'.format(-1, '', 255))) >> (round(abs((1e10 ** -1))) % 9))
    else:
        class Err12(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err12(chr(3033), 1)
            except Err12 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
