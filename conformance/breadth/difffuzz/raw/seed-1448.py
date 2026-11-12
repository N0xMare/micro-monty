# difffuzz divergence, seed 1448
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u2e75']
#   mm:      ['⹵']
# signature: CPython: ['\u#e#'] | mm: ['⹵']
import math
try:
    def f20(p0, p1):
        data = chr(11893).splitlines()
        print(sorted(data, key=lambda q: str(q)[::-1], reverse=False))
        def f21(p0, p1):
            if v10.islower():
                print('v25', v25)
    print("f20(f20(min([5, 5, -1, -3]), list(map(lambda q: q * 3 + 1, [42]))).ljust(10, chr(45)).index(repr('\\u00df'.capitalize()).partition(',')[0]), [ord(c) for c in (f'{[7, -3]!r} {-2.25:.0f} and {2.5:.3e}|' + repr(['mississippi']))])", f20(f20(min([5, 5, -1, -3]), list(map(lambda q: q * 3 + 1, [42]))).ljust(10, chr(45)).index(repr('\u00df'.capitalize()).partition(',')[0]), [ord(c) for c in (f'{[7, -3]!r} {-2.25:.0f} and {2.5:.3e}|' + repr(['mississippi']))]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
