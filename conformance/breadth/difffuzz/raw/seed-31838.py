# difffuzz divergence, seed 31838
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError 0000᥵ ('0000\u1975',)
#   mm:      caught ValueError 0000᥵ ('0000᥵',)
# signature: CPython: caught ValueError #᥵ ('#\u#',) | mm: caught ValueError #᥵ ('#᥵',)
import math
try:
    n25 = 3
    while n25 > 0:
        class Ctx26:
            def __init__(self, name):
                self.name = name
            def __enter__(self):
                return self
            def __exit__(self, kind, value, tb):
                return False
        with Ctx26("a") as x, Ctx26("b"):
            pass
        try:
            raise ValueError(chr(6517).zfill(5))
        except Exception as e:
            print("caught", type(e).__name__, e, e.args)
            class C31:
                def __init__(self, v, w=0):
                    self.w = w
            def f32(p0, p1, p2):
                return '{}-{!r}-{:>4}'.format(5, p1.center(2, chr(42)).capitalize().partition(',')[2], sorted([(p0 + i35) for i35 in range(1, 9) if i35 % 2], reverse=True)[pow(pow(v33, 25, 7), 35, 2 ** 61 - 1)])
            print("f32(int(('AbC' + 'hello').swapcase()[sum(list(reversed([42, 7, -17])))]), ''.lstrip('a').expandtabs()[((i29 if True else -3) >> (''.count('a') % 9))].partition(',')[1], list(reversed([(dict(sorted({'k0': 10, 'k1': 1}.items(), key=lambda kv: kv[1]))['k2'] + i36) for i36 in range(8)])))", f32(int(('AbC' + 'hello').swapcase()[sum(list(reversed([42, 7, -17])))]), ''.lstrip('a').expandtabs()[((i29 if True else -3) >> (''.count('a') % 9))].partition(',')[1], list(reversed([(dict(sorted({'k0': 10, 'k1': 1}.items(), key=lambda kv: kv[1]))['k2'] + i36) for i36 in range(8)]))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
