# difffuzz divergence, seed 42622
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err3 "'7123x\\nyAbCx\\ny  pad  x\\ny3.5\\u0ffc10123x\\nyAbCx\\ny  pad  x\\ny3.5\\u0ffc1123x\\nyAbCx\\ny  pad  x\\ny3.5\\u0ffc100123x\\nyAbCx\\ny  pad  x\\ny3.5\\u0ffc2' and sorted(sorted({-17, 2, 10, 100}), key=lambda q: (q % 3, -q))=[100, 10, -17, 2]0.000000%" 7 wrapped
#   mm:      Err3 "'7123x\\nyAbCx\\ny  pad  x\\ny3.5࿼10123x\\nyAbCx\\ny  pad  x\\ny3.5࿼1123x\\nyAbCx\\ny  pad  x\\ny3.5࿼100123x\\nyAbCx\\ny  pad  x\\ny3.5࿼2' and sorted(sorted({-17, 2, 10, 100}), key=lambda q: (q % 3, -q))=[100, 10, -17, 2]0.000000%" 7 wrapped
# signature: CPython: Err# "'#x\\nyAbCx\\ny  pad  x\\ny#.#\\u#ffc#x\\nyAbCx\\ny  pad  x\\ny#.#\\u#ffc#x\\nyAbCx\\ny  pad   | mm: Err# "'#x\\nyAbCx\\ny  pad  x\\ny#.#࿼#x\\nyAbCx\\ny  pad  x\\ny#.#࿼#x\\nyAbCx\\ny  pad  x\\ny#.#࿼#x\
import math
try:
    def f2(p0, p1, *rest, kw=2):
        class Err3(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err3(f'{p1!r:>12} and {sorted(sorted({-17, 2, 10, 100}), key=lambda q: (q % 3, -q))=}{divmod(sum(p0), +1000)[0]:%}', 7)
            except Err3 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
            total = start
            def step(by=1):
                return total
    print("f2([ord(c) for c in ('AbC' + 'mississippi').join('x\\ny'.split(' '))][:-4], ('x\\ny'.join(['123', 'AbC', '  pad  ', '3.5']).center(0, chr(45)) + chr(4092)).join([str(q) for q in [7, 10, 1, 100, 2]]))", f2([ord(c) for c in ('AbC' + 'mississippi').join('x\ny'.split(' '))][:-4], ('x\ny'.join(['123', 'AbC', '  pad  ', '3.5']).center(0, chr(45)) + chr(4092)).join([str(q) for q in [7, 10, 1, 100, 2]])))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
