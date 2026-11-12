# difffuzz divergence, seed 6148
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'1': 1, '2': 2, '3': 3, '4': 4, '5': 5, '6': 6, '7': 7, '\u0bdc': 1}
#   mm:      {'1': 1, '2': 2, '3': 3, '4': 4, '5': 5, '6': 6, '7': 7, '௜': 1}
# signature: CPython: {'#': #, '#': #, '#': #, '#': #, '#': #, '#': #, '#': #, '\u#bdc': #} | mm: {'#': #, '#': #, '#': #, '#': #, '#': #, '#': #, '#': #, '௜': #}
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i4 in range(5):
        for i6 in lst0:
            import asyncio
            from contextlib import aclosing
            async def async_cleanup7():
                log = []
                class CM:
                    async def __aenter__(self):
                        return 3
                    async def __aexit__(self, kind, value, tb):
                        return False
                async def values():
                    try:
                        for i in range(0):
                            yield i
                    finally:
                        log.append("closed")
                try:
                        async with aclosing(values()) as stream:
                            async for item in stream:
                                    return log
                            else:
                                log.append("exhausted")
                except ValueError:
                    log.append("caught")
        def f8(p0, p1, p2):
            for i9 in range(0, 9):
                print(({str(q): q for q in [((i4 >> (5 % 9)) + i10) for i10 in range(1, 8)]} | {k11: len(k11) for k11 in list(chr(3036))}), sep='')
        print("f8(list(filter(lambda q: q > 0, lst0)), 'caf\\u00e9'[i6].title().replace(('x\\ny'.lower() * (f8(lst0, -17, '  pad  ') % 4)), '123'.swapcase().join('mississippi'.split(' '))).rfind('Hello World'.replace('x\\ny', '\\u00df')[::1][5:]), 'mississippi'[i4].expandtabs().replace('Hello World', str(2.5)[:-3]).strip('\\U0001f600 ok'[i6].join(['', '-42', '\\U0001f600 ok', '  pad  ']).center(7, chr(45))))", f8(list(filter(lambda q: q > 0, lst0)), 'caf\u00e9'[i6].title().replace(('x\ny'.lower() * (f8(lst0, -17, '  pad  ') % 4)), '123'.swapcase().join('mississippi'.split(' '))).rfind('Hello World'.replace('x\ny', '\u00df')[::1][5:]), 'mississippi'[i4].expandtabs().replace('Hello World', str(2.5)[:-3]).strip('\U0001f600 ok'[i6].join(['', '-42', '\U0001f600 ok', '  pad  ']).center(7, chr(45)))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
