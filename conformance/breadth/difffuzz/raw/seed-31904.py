# difffuzz divergence, seed 31904
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: 1-'******\u0c49'-  -2v23 {'k0': 10, 'k1': 255}
#   mm:      1-'******౉'-  -2v23 {'k0': 10, 'k1': 255}
# signature: CPython: #-'******\u#c#'-  -#v# {'k#': #, 'k#': #} | mm: #-'******౉'-  -#v# {'k#': #, 'k#': #}
import math
try:
    n2 = 6
    while n2 > 0:
        n2 -= 1
        class Ctx3:
            def __init__(self, name):
                self.name = name
        try:
            try:
                raise Err4(chr(4122), 1)
            except Err4 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print('{}-{!r}-{:>4}'.format(+min([a * b for a, b in zip(lst0, lst0)]), chr(3145).rjust(7, chr(42)), (-1 ^ (len('a') ** (n2 % 7)))), end='')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
        v23 = {'k0': 10, 'k1': 255}
        print('v23', v23)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
