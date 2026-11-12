# difffuzz divergence, seed 2242
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err3 '\u206c' 2 wrapped
#   mm:      Err3 '⁬' 2 wrapped
# signature: CPython: Err# '\u#c' # wrapped | mm: Err# '⁬' # wrapped
import math
try:
    def f2(p0, p1, p2):
        class Err3(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err3(chr(8300), 2)
            except Err3 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
    print('f2(pow((int(--2.25) ** (2 % 7)), 12, -5), list(map(lambda q: q * 3 + 1, list(filter(lambda q: q > 2, [(math.trunc(abs(7) % 30) + i4) for i4 in sorted([42, 3, -17], key=abs) if i4 % 4])))), chr(10958))', f2(pow((int(--2.25) ** (2 % 7)), 12, -5), list(map(lambda q: q * 3 + 1, list(filter(lambda q: q > 2, [(math.trunc(abs(7) % 30) + i4) for i4 in sorted([42, 3, -17], key=abs) if i4 % 4])))), chr(10958)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
