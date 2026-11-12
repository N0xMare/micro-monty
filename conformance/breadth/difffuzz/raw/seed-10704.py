# difffuzz divergence, seed 10704
# CPython: exit 0, last stderr line:   return n
# mm:      exit 0, last stderr line:   return n
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u0e65'
#   mm:      EXC ValueError invalid literal for int() with base 36: '๥'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#' | mm: EXC ValueError invalid literal for int() with base #: '๥'
import math
try:
    class C3:
        def __init__(self, v, w=0):
            self.w = w
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n5 = 4
    while n5 > 0:
        n5 -= 1
        class Ctx6:
            def __init__(self, name):
                self.name = name
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f8(p0):
        if '\u00df'.isalpha():
            class Ctx9:
                def __init__(self, name):
                    self.name = name
                def __enter__(self):
                    return self
                def __exit__(self, kind, value, tb):
                    return True
            with Ctx9("a") as x, Ctx9("b"):
                print(x.name * 2)
            class C10(C3):
                def __init__(self, v, w=0):
                    self.w = w
            objs = [C10((int(chr(3685), 36) ** (((-5 << ((n5 ** (-3 % 7)) % 70)) >> (((p0 ** (1000 % 7)) // n5) % 9)) % 7)), 4), C10(abs(p0).bit_length(), -4), C10(dict(sorted(({'3.5': 42, 'mississippi': 255} | {k11: len(k11) for k11 in []}).items(), key=lambda kv: kv[1]))['zz'], -1), C10.make(5)]
            class Ctx12:
                def __init__(self, name):
                    self.name = name
            try:
                raise ValueError(str(int(repr({'k0': -1, 'k1': 100})[:], 36)))
            finally:
                print("finally", (len(['hello', 'Hello World', 'x\ny']) >> (divmod(sum(([10, 0, -17, 1000] + [1000, 10, 255, 1000])), min(list(reversed([1000, 42, -17, 10]))))[1] % 9)))
                if n <= 1:
                    return n
    print("f8((list(filter(lambda q: q > 3, ([0, -17, 255] + [2, 3, 0])))[round((1e-05 + 0.1))] << ((f'{[1, -3]!r}|{None!r}'.find('123'[-1:8]) ** (f8((n5 ** (n5 % 7))) % 7)) % 70)))", f8((list(filter(lambda q: q > 3, ([0, -17, 255] + [2, 3, 0])))[round((1e-05 + 0.1))] << ((f'{[1, -3]!r}|{None!r}'.find('123'[-1:8]) ** (f8((n5 ** (n5 % 7))) % 7)) % 70))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
