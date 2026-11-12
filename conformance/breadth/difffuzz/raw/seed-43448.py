# difffuzz divergence, seed 43448
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0bbc'
#   mm:      EXC ValueError could not convert string to float: '஼'
# signature: CPython: EXC ValueError could not convert string to float: '\u#bbc' | mm: EXC ValueError could not convert string to float: '஼'
import math
try:
    def f4(p0, p1, p2):
        class Err5(ValueError):
            def __init__(self, msg, code=0):
                self.code = code
        try:
            try:
                raise Err5(f'{+-1e-05:.10g}', 4)
            except Err5 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
        v6 = ([len(w) for w in sorted(['AbC', '3.5'], key=lambda w: w[::-1])][:] >= sorted({42, -3, -3, -3, 255}))
        return round(float(chr(3004)))
    print('f4(sorted([((round(1e10) >> (int(1e10) % 9)) + i7) for i7 in [a * b for a, b in zip([1], [-3, 3, 42, 255, 100])][:-4:-2]], key=abs), sorted([i + v for i, v in enumerate([((10 if True else 100) + i8) for i8 in range(5)], 1)]), round(math.log2(math.e)))', f4(sorted([((round(1e10) >> (int(1e10) % 9)) + i7) for i7 in [a * b for a, b in zip([1], [-3, 3, 42, 255, 100])][:-4:-2]], key=abs), sorted([i + v for i, v in enumerate([((10 if True else 100) + i8) for i8 in range(5)], 1)]), round(math.log2(math.e))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
