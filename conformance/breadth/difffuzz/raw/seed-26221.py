# difffuzz divergence, seed 26221
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1ad1'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᫑'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ad#' | mm: EXC ValueError invalid literal for int() with base #: '᫑'
import math
try:
    if bool(repr('Hello World'.split(' ')).lstrip(('-42' * (42 % 4)).strip())):
        n14 = 3
        while n14 > 0:
            class Err15(KeyError):
                    self.code = code
            try:
                try:
                    raise Err15('x\ny', 4)
                except Err15 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
        match (1, "x"):
            case int() | float():
                print("number")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    if any([n14, int(chr(6865)), divmod('a'.rfind('Hello World'), abs(n14).bit_length())[0]]):
        match '{}-{!r}-{:>4}'.format(int(str(tuple(sorted({}.items()))), 2), '%e' % (float('  pad  '.partition(' ')[1]), max([ord(c) for c in '\U0001f600 ok']),), (100 ** (round(math.tau) % 7))):
            case (a, "x"):
                print("pair", a)
        def make21(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
