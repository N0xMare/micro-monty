# difffuzz divergence, seed 1172
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u05ca'
#   mm:      EXC ValueError invalid literal for int() with base 10: '׊'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ca' | mm: EXC ValueError invalid literal for int() with base #: '׊'
import math
try:
    def f22(p0, p1):
        class Err23(KeyError):
                self.code = code
        try:
            try:
                raise Err23(repr((math.log(math.atan(7.125)) ** 0.5)), 1)
            except Err23 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
            def __init__(self, name):
                self.name = name
    print('f22(str([(int(chr(1482)) + i25) for i25 in range(1)]), list(filter(lambda q: q > 1, list(reversed([a * b for a, b in zip(sorted(v16), (v12 * 1))])))))', f22(str([(int(chr(1482)) + i25) for i25 in range(1)]), list(filter(lambda q: q > 1, list(reversed([a * b for a, b in zip(sorted(v16), (v12 * 1))]))))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
