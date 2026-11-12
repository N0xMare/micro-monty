# difffuzz divergence, seed 36347
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u2fed'
#   mm:      EXC ValueError invalid literal for int() with base 16: '⿭'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fed' | mm: EXC ValueError invalid literal for int() with base #: '⿭'
import math
try:
    if bool({'k0': -3, 'k1': 3}):
        v5 = [len(w) for w in [str(q) for q in ([i + v for i, v in enumerate([], 1)] + list(filter(lambda q: q > 4, [42])))]]
        try:
            print(int('hello'))
        except KeyError as e:
            print("caught", type(e).__name__, e, e.args)
        finally:
            print("finally", int(chr(12269), 16))
        def rec9(n):
            if n <= 1:
                return n
        try:
            raise ValueError('3.5')
        except LookupError as e:
            print("caught", type(e).__name__, e, e.args)
        def make16(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
