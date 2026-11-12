# difffuzz divergence, seed 35182
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: "3-'\\u1a7d'- 100"
#   mm:      EXC ValueError invalid literal for int() with base 16: "3-'᩽'- 100"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "#-'\\u#a#d'- #" | mm: EXC ValueError invalid literal for int() with base #: "#-'᩽'- #"
import math
try:
    def f10(p0):
        def risky():
            try:
                print((((10 ** 30 + 7 << ((p0 if True else p0) % 70)) ** (pow((p0 << (p0 % 70)), 20, 1000) % 7)) ^ sum([a * b for a, b in zip(sorted([], reverse=True), ([3, 1, 1, 100] * 2))])).nope)
            except LookupError as e:
                print("caught", type(e).__name__, e, e.args)
            finally:
                print("finally", (f'{[-1, -3, 42, 1]!r} '.index('123'.replace('hello', '\U0001f600 ok')[round(-0.0)]) ** (int('{}-{!r}-{:>4}'.format(max([2, 3, 1, 3, -1]), chr(6781), abs(p0).bit_length()), 16) % 7)))
        print(risky())
    print('f10((1 << 100) - 1)', f10((1 << 100) - 1))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
