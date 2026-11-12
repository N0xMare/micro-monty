# difffuzz divergence, seed 33090
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u1c38'
#   mm:      EXC ValueError invalid literal for int() with base 0: '᰸'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#c#' | mm: EXC ValueError invalid literal for int() with base #: '᰸'
import math
try:
    if ((int(chr(7224), 0) < (v20 | {'k0': 1000, 'k1': -1})['k2'] <= n3) or bool('  pad  '.zfill(0)[:5])):
        if (repr((2.5 ** 2)).isupper() and (-v20['k0'] in list(map(lambda q: q * 0 + 1, [])))):
            async def async_cleanup24():
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
            try:
                try:
                    raise Err25('tab\there'.split(' ')[0].strip(), 1)
                except Err25 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
