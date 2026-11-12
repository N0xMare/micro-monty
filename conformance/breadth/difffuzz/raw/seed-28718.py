# difffuzz divergence, seed 28718
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u2d6b'
#   mm:      EXC ValueError invalid literal for int() with base 36: '⵫'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#d#b' | mm: EXC ValueError invalid literal for int() with base #: '⵫'
import math
try:
    n1 = 2
    while n1 > 0:
        for i2 in range(1):
            if i2 == 2:
                continue
            lst0 = [3, 1, 2]
            for i5 in range(3, 6):
                print('v6', v6)
                def __init__(self, msg, code=0):
                    self.code = code
            try:
                try:
                    raise Err8('Hello World', 0)
                except Err8 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f14(p0):
        class C15:
            def __init__(self, v, w=0):
                self.w = w
        objs = [C15(int(chr(11627), 36), 4), C15(math.ceil(abs(2) % 30), 0), C15(pow(sum(list(map(lambda q: q * -1 + 1, [i + v for i, v in enumerate(p0, 1)]))), 6, 1000), 0), C15.make(2)]
    print('f14(lst0)', f14(lst0))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
