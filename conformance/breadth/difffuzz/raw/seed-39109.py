# difffuzz divergence, seed 39109
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '0000000\u0a7f'
#   mm:      EXC ValueError invalid literal for int() with base 16: '0000000੿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#a#f' | mm: EXC ValueError invalid literal for int() with base #: '#੿'
import math
try:
    if (not (divmod(~0, abs(5).bit_length())[1] < int(float(0)) <= (~-3 >> (abs(7).bit_length() % 9)))):
        if (w := (int(chr(2687).zfill(8), 16) >> (divmod(math.trunc(abs((0 | 255)) % 30), round(math.atan(-2.25)))[1] % 9))) > 14:
            print("walrus", w)
            def __init__(self, msg, code=0):
                self.code = code
        try:
            try:
                raise Err17('hello'.swapcase().removeprefix(f'{10:#b}|{None:<8} {0:.2f}'.split(' ')[-1]), 7)
            except Err17 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
            def __init__(self, name):
                self.name = name
        def make19(start):
            total = start
            def step(by=1):
                return total
        try:
            try:
                raise Err20(('tab\there'[5].zfill(7).removeprefix('\U0001f600 ok'.join(['\U0001f600 ok', '\u00df', 'hello'])[(1 % 5)]) * (abs(max([10, 3, -3, 0, -1])).bit_length() % 4)), 0)
            except Err20 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
