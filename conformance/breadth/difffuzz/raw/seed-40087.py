# difffuzz divergence, seed 40087
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u137e'
#   mm:      EXC ValueError invalid literal for int() with base 16: '፾'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e' | mm: EXC ValueError invalid literal for int() with base #: '፾'
import math
try:
    def f5(p0):
        if 'x\ny'.removeprefix('\u00df').expandtabs().swapcase().isdigit():
            print(sorted({c for c in str(sorted({-3, 2})) if c.isalpha()}))
        elif (divmod((round(1e-05) * int(p0, 16)), ((100 >> (i2 % 9)) if (i2 < -3 <= i2) else math.gcd(1.5)))[0] in list(map(lambda q: q * -3 + 1, [(pow(i2, 6, -5) + i8) for i8 in list(reversed(lst0)) if i8 % 4]))):
            class Err9(LookupError):
                    self.code = code
            try:
                try:
                    raise Err9(repr(v6).removeprefix('  pad  '.rjust(4, chr(45)).center(9, chr(42))), 3)
                except Err9 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
        match (1, "x"):
            case {}:
                print("any mapping")
    print('f5(chr(4990))', f5(chr(4990)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
