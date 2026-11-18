# A generator, coroutine or async generator method called on an object of
# another kind (`type(gen()).close(obj)`) raises TypeError, as CPython's
# descriptor does, and leaves the object as it was: the methods write the
# slots of their self, so a dict, tuple or instance passed as self had
# them overwritten. A generator of the wrong kind, a subclass instance and
# asyncio.sleep() count as another kind or as a coroutine as in CPython.
import asyncio


def gen():
    yield 1


async def co():
    return 1


async def ag():
    yield 1


c0 = co()
G, C, A = type(gen()), type(c0), type(ag())
c0.close()


class K:
    def __init__(s):
        s.a = 1


class D(dict):
    pass


class E(ValueError):
    pass


def objs():
    return [('dict', {'a': 1, 'b': 2}), ('tuple', (1, 2, 3)), ('inst', K()),
            ('dict subclass', D(a=1)), ('exception', E(1, 2)), ('set', {1, 2}),
            ('builtin', len), ('generator', gen()), ('coroutine', co()),
            ('async generator', ag()), ('sleep', asyncio.sleep(0))]


def show(o):
    try:
        if isinstance(o, K):
            return sorted(vars(o).items())
        if isinstance(o, BaseException):
            return o.args
        if isinstance(o, (G, C, A)) or type(o).__name__ == 'coroutine':
            return type(o).__name__
        return repr(o)
    except BaseException as e:
        return 'broken: ' + type(e).__name__


def done(o):
    if isinstance(o, C) or type(o).__name__ == 'coroutine':
        o.close()


METHODS = ((G, ('close', 'send', '__next__', '__iter__')),
           (C, ('close', 'throw', 'send', '__await__')),
           (A, ('asend', '__anext__', '__aiter__')))


async def main():
    for T, ms in METHODS:
        for m in ms:
            for name, obj in objs():
                if isinstance(obj, T) or (T is C and name == 'sleep'):
                    done(obj)
                    continue
                try:
                    f = getattr(T, m)
                    if m in ('throw', 'send', 'athrow', 'asend'):
                        r = f(obj, ValueError('x'))
                    else:
                        r = f(obj)
                    if T is A and m != '__aiter__':
                        r = await r
                    res = 'returned ' + type(r).__name__
                except BaseException as e:
                    res = f'{type(e).__name__}: {e}'
                print(T.__name__, m, 'on', name, '|', res, '|', show(obj))
                done(obj)


asyncio.run(main())
