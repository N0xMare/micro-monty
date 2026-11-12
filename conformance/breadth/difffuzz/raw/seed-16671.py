# difffuzz divergence, seed 16671
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['mississippi\u0de3-42\u0de3ß\u0de3Hello World']
#   mm:      ['mississippi෣-42෣ß෣Hello World']
# signature: CPython: ['mississippi\u#de#-#\u#de#ß\u#de#Hello World'] | mm: ['mississippi෣-#෣ß෣Hello World']
import math
try:
    for i24 in [ord(c) for c in 'tab\there'.split(' ')[-1].join(['Hello World', '\u00df', 'x\ny', 'a']).strip()]:
        if '  pad  '.replace('3.5', 'hello').replace('x\ny'.replace('a', '-42'), 'x\ny'.strip()).zfill(4).isidentifier():
            class Ctx25:
                def __init__(self, name):
                    self.name = name
            match None:
                case str(s) if len(s) > 3:
                    print("long", s[:3])
            def gen28(n):
                for k in range(n):
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
                                    raise ValueError("stop")
                            else:
                                log.append("exhausted")
                except ValueError:
                    log.append("caught")
        else:
            match None:
                case str(s) if len(s) > 3:
                    print("long", s[:3])
            data = chr(3555).join(list(dict(zip(['mississippi', '-42', '\u00df', 'Hello World'], [42, -3, 10, 5])))).splitlines()
            print(sorted(data, key=str, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
