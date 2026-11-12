# difffuzz divergence, seed 26349
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0b4f'
#   mm:      EXC ValueError invalid literal for int() with base 10: '୏'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#f' | mm: EXC ValueError invalid literal for int() with base #: '୏'
import math
try:
    def f14(p0, p1, *rest, kw=3):
        match None:
            case int(n) if n > 10:
                print("big", n)
    print("f14((int(chr(2895)) >> (pow((2 - int('caf\\u00e9', 0)), 40, 2 ** 61 - 1) % 9)), repr(divmod(int('-42'.join([]), 36), i6)))", f14((int(chr(2895)) >> (pow((2 - int('caf\u00e9', 0)), 40, 2 ** 61 - 1) % 9)), repr(divmod(int('-42'.join([]), 36), i6))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
