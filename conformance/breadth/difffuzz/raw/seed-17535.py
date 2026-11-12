# difffuzz divergence, seed 17535
# CPython: exit 0, last stderr line:   break
# mm:      exit 0, last stderr line:   break
# first difference (stdout line 3, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0e7e'
#   mm:      EXC ValueError invalid literal for int() with base 10: '๾'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#e#e' | mm: EXC ValueError invalid literal for int() with base #: '๾'
import math
try:
    def f1(p0, p1, p2):
        for i2 in list({'k0': 3}.values()):
            def gen3(n):
                for k in range(n):
                    if k % 2 == 0:
                        yield k * 0
            it = gen3(5)
        return chr(3710)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i16 in range(3, 6):
        try:
            print({v7: (((i16 << (i16 % 70)) if isinstance(i16, str) else [255, 0, 1, 10][i16]) if ('hello'.removesuffix('caf\u00e9') < 'x\ny'.join(['', 'Hello World'])) else pow(int('', 16), 35, 7))}[chr(8566).join(sorted(list(f'{v7:>8}|{1e-05:e} '), key=lambda w: (-len(w), w)))])
        finally:
            print("finally", round(round((-7.125 // abs(0.1)), 4)))
            if i18 == 2:
                break
            try:
                print(math.sqrt(((float('-42') - (+2.5 / abs(1e-05))) ** 0.5)))
            except ValueError as e:
                print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    if any([a * b for a, b in zip(list(reversed([i16, 1, 0, 1000])), [])]):
        async def async_cleanup26():
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
    else:
        print((int(f1('\u00df', [100, 1000, 100, -1], 'caf\u00e9').partition(' ')[0]) / {'k0': 1, 'k1': 10, 'k2': -17, 'k3': -3}['k1']), sep='mississippi')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
