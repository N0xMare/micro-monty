# difffuzz divergence, seed 14489
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: []tab	hereErr18 '\u0b7e'-   1 6 wrapped
#   mm:      []tab	hereErr18 '୾'-   1 6 wrapped
# signature: CPython: []tab	hereErr# '\u#b#e'-   # # wrapped | mm: []tab	hereErr# '୾'-   # # wrapped
import math
try:
    print(list(filter(lambda q: q > 4, sorted(list(range(5, 8, -1)), key=lambda q: (q % 3, -q)))), end='tab\there')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class Err18(ValueError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err18('{}-{!r}-{:>4}'.format(('tab\there'.rfind('  pad  ') << (pow(7, 3, 7) % 70)), chr(2942), pow(pow(1, 9, 2 ** 61 - 1), 0, 1000))[3::1], 6)
        except Err18 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
