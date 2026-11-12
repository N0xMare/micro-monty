# difffuzz divergence, seed 20970
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: "'\\u17ed'"
#   mm:      EXC ValueError invalid literal for int() with base 36: "'៭'"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "'\\u#ed'" | mm: EXC ValueError invalid literal for int() with base #: "'៭'"
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i18 in sorted(list(map(lambda q: q * 0 + 1, ([ord(c) for c in 'AbC'] + [len(w) for w in ['\u00df', 'Hello World']]))), key=str):
        if i18 == 0:
            break
        try:
            raise KeyError([i + v for i, v in enumerate(sorted([], key=abs), 0)][(2 ** 64 // pow((i18 ** (100 % 7)), 5, 2 ** 61 - 1))])
        finally:
            print("finally", list(filter(lambda q: q > 0, list(dict(zip(['caf\u00e9', 'AbC', '3.5'], lst0)).values())))[int(repr(chr(6125)), 36)])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
