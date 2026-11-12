# difffuzz divergence, seed 33337
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0fec'
#   mm:      EXC ValueError invalid literal for int() with base 16: '࿬'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fec' | mm: EXC ValueError invalid literal for int() with base #: '࿬'
import math
try:
    if ('{}-{!r}-{:>4}'.format(len(sorted(['a,b,,c'], key=lambda w: w[::-1])), 'a,b,,c'.split(None)[-1].capitalize(), sum(list(filter(lambda q: q > 2, [255, 7])))) < '{}-{!r}-{:>4}'.format(int(chr(4076), 16), '3.5'.removeprefix('hello').removesuffix('%-6s|' % (v1, v1,)), '3.5'.lstrip('a').find('-42'.join(['mississippi', 'hello', '\u00df'])))):
        print([0, -17, 100], sep='Hello World')
        match None:
            case int() | float():
                print("number")
        while n15 > 0:
            if (f10(f'{-3:.2f}', (round(1.5) if (not True) else 'caf\u00e9'.count('a,b,,c'))) == dict(zip(sorted(['AbC', 'Hello World', '123', 'tab\there'], key=str.lower), [ord(c) for c in '']))['k1']):
                print('i2', i2)
        match tuple(sorted([i + v for i, v in enumerate([len(w) for w in ['tab\there']], 2)], key=abs)):
            case list(l):
                print("list", len(l))
        if all([('hello'.removeprefix('\u00df').rfind('\U0001f600 ok') + i18) for i18 in range(3)]):
            if False:
                print(~n15)
            def rec27(n):
                if n <= 1:
                    return n
            if ((((0.0 % v1) ** 3) >= round(v1)) or False):
                print(sorted(list('3.5'.zfill(3).zfill(4)), key=len), end='a,b,,c')
                def __init__(self, v, w=0):
                    self.w = w
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
