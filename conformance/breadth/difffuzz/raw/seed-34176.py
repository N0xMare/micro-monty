# difffuzz divergence, seed 34176
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: ['000\u18ae']
#   mm:      ['000᢮']
# signature: CPython: ['#\u#ae'] | mm: ['#᢮']
import math
try:
    if '123'.expandtabs().partition(' ')[2][7::2].isidentifier():
        try:
            print(float(str([len(w) for w in ['3.5', '123', '3.5']]).lower()[:-3:-1]))
        except (TypeError, ValueError) as e:
            print("caught", type(e).__name__, e, e.args)
        match [(-((7 if False else 3) << (-3 % 70)) + i2) for i2 in range(5) if i2 % 2]:
            case {}:
                print("any mapping")
    elif (2 in tuple(list(reversed([3, 255, 2, 42])))):
        v4 = chr(6318).zfill(4)
        async def async_cleanup6():
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
        match (dict(zip(v5, (list(range(-1, 2, -2)) * 2))) | dict(sorted({k8: len(k8) for k8 in list('hello')}.items(), key=lambda kv: kv[1]))):
            case [x]:
                print("one", x)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
    data = sorted(sorted(v4.swapcase().splitlines(), key=lambda w: w[::-1]), key=str.lower)
    print(sorted(data, key=lambda q: str(q)[::-1], reverse=True))
