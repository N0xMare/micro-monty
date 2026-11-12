# difffuzz divergence, seed 18337
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: [-3, -1, 100]caféEXC ValueError invalid literal for int() with base 16: '\u0ad2'
#   mm:      [-3, -1, 100]caféEXC ValueError invalid literal for int() with base 16: '૒'
# signature: CPython: [-#, -#, #]caféEXC ValueError invalid literal for int() with base #: '\u#ad#' | mm: [-#, -#, #]caféEXC ValueError invalid literal for int() with base #: '૒'
import math
try:
    if bool(+len(repr('-42'))):
        def f23(p0, p1, *rest, kw=2):
            print(sorted({-1, 100, -3}), sep='\u00df', end='caf\u00e9')
            return int(chr(2770), 16)
        print("f23(list(filter(lambda q: q > -1, sorted(({-1, 3, 7} - {-17, 2, 7, 1, 1000})))), list(filter(lambda q: q > 3, ([len(['a'])] + ([len(w) for w in ['\\U0001f600 ok', 'Hello World', 'a,b,,c']] * 1)))))", f23(list(filter(lambda q: q > -1, sorted(({-1, 3, 7} - {-17, 2, 7, 1, 1000})))), list(filter(lambda q: q > 3, ([len(['a'])] + ([len(w) for w in ['\U0001f600 ok', 'Hello World', 'a,b,,c']] * 1))))))
        while n24 > 0:
            def make25(start):
                total = start
                def step(by=1):
                    return total
        match ((-(10 ** 25), f'{3.0:.0f}{[7, 100, 100, 5]} {None} ', '-42'.title(), )[::2] + ()):
            case list(l):
                print("list", len(l))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
