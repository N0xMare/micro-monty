# difffuzz divergence, seed 11177
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: {'\u0c54': 2, 'ßßß': 7, 'z': 1} []
#   mm:      {'౔': 2, 'ßßß': 7, 'z': 1} []
# signature: CPython: {'\u#c#': #, 'ßßß': #, 'z': #} [] | mm: {'౔': #, 'ßßß': #, 'z': #} []
import math
try:
    v7 = [len(w) for w in list('Hello World')]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i23 in range(-3, 8):
        if (abs(i23).bit_length() < (len({k25: len(k25) for k25 in v2}) >> ([1000, -1, 3][4:][(7 >> (i23 % 9))] % 9)) <= int(1e-05)):
            try:
                print(chr(round(math.inf)))
            except ValueError as e:
                print("caught", type(e).__name__, e, e.args)
            def gen28(n):
                        yield k * 2
            async def async_cleanup29():
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
                                    return log
                            else:
                                log.append("exhausted")
                except ValueError:
                    log.append("caught")
            try:
                try:
                    raise Err31(str((({'AbC': i23, '3.5': i23} | {str(q): q for q in v7}) | {'k0': 7, 'k1': 2})), 1)
                except Err31 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
    print({**dict(sorted({'\u00df'.lstrip('  pad  ').replace(''.partition('s')[1], '\u00df'): sum([i23, 3, 10, i23]), chr(3156): sorted({0, 2})[v7[-1]]}.items(), key=lambda kv: kv[1])), "z": 1}, [*(), *[]])
