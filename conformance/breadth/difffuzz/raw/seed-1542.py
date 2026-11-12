# difffuzz divergence, seed 1542
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u1a7d'
#   mm:      EXC ValueError invalid literal for int() with base 0: '᩽'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#a#d' | mm: EXC ValueError invalid literal for int() with base #: '᩽'
import math
try:
    def f2(p0, p1, p2, *rest, kw=5):
        match ()[:2:1]:
            case str(s) if len(s) > 3:
                print("long", s[:3])
    print("f2((str({'k0': 7, 'k1': 10, 'k2': 10, 'k3': 0}).join(list('hello')).strip() * (divmod(int(chr(6781), 0), 2 ** 64)[1] % 4)), len(str(-0.0)[:1].removesuffix('tab\\there'.zfill(3).replace('  pad  '.replace('123', 'tab\\there'), chr(6039)))), int(repr((+1e10 ** 3))))", f2((str({'k0': 7, 'k1': 10, 'k2': 10, 'k3': 0}).join(list('hello')).strip() * (divmod(int(chr(6781), 0), 2 ** 64)[1] % 4)), len(str(-0.0)[:1].removesuffix('tab\there'.zfill(3).replace('  pad  '.replace('123', 'tab\there'), chr(6039)))), int(repr((+1e10 ** 3)))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
