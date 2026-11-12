# difffuzz divergence, seed 13649
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u173f'
#   mm:      EXC ValueError invalid literal for int() with base 16: '᜿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f' | mm: EXC ValueError invalid literal for int() with base #: '᜿'
import math
try:
    n1 = 6
    while n1 > 0:
        n1 -= 1
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f6(p0, p1, p2):
        def f7(p0, p1):
            async def async_cleanup8():
                class CM:
                    async def __aenter__(self):
                        return 3
                async def values():
                    try:
                            yield i
                    finally:
                        log.append("closed")
                try:
                            async for item in stream:
                                    continue
                            else:
                                log.append("exhausted")
                except ValueError:
                    log.append("caught")
        print("f7((int(repr(f'{p2!r} and '), 2) if '{}-{!r}-{:>4}'.format(p1, 'mississippi', n1).split('s')[-2].isalpha() else int(chr(5951), 16)), repr(('mississippi'.zfill(6).upper() + '')))", f7((int(repr(f'{p2!r} and '), 2) if '{}-{!r}-{:>4}'.format(p1, 'mississippi', n1).split('s')[-2].isalpha() else int(chr(5951), 16)), repr(('mississippi'.zfill(6).upper() + ''))))
    print("f6(math.factorial(abs(abs('AbC'[2].find('caf\\u00e9'.removesuffix('a'))).bit_length()) % 30), abs(-min(sorted([1, 5, 7], key=str))).bit_length(), (([len(w) for w in sorted(['\\U0001f600 ok', ''], key=str.lower)] + list(map(lambda q: q * -2 + 1, list(filter(lambda q: q > 0, [1, 7, 10, 0, 1000]))))) * 3))", f6(math.factorial(abs(abs('AbC'[2].find('caf\u00e9'.removesuffix('a'))).bit_length()) % 30), abs(-min(sorted([1, 5, 7], key=str))).bit_length(), (([len(w) for w in sorted(['\U0001f600 ok', ''], key=str.lower)] + list(map(lambda q: q * -2 + 1, list(filter(lambda q: q > 0, [1, 7, 10, 0, 1000]))))) * 3)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
