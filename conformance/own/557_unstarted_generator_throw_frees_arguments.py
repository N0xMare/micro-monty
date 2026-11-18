# A generator that has not started and is thrown into is over: the
# exception goes to the caller and the bound arguments go with the frame,
# as close() already did. So do athrow() and aclose() on an async
# generator that has not started. Each shape binds 150 MB as an argument
# (in 50 MB blocks, under the object size cap) and keeps the generator,
# then makes 150 MB again under the 256 MiB limit; it fits only if the
# argument is not held. python3.14 prints "ok" for each.

import asyncio

N = 50_000_000


def load():
    return [bytearray(N) for _ in range(3)]


def gen(big):
    yield len(big)


async def agen(big):
    yield len(big)


def check(label, act):
    try:
        g = act()
        print(label, 'ok', len(load()))
        return g
    except MemoryError:
        print(label, 'MemoryError: the argument is still held')


def thrown():
    g = gen(load())
    try:
        g.throw(KeyError('k'))
    except KeyError as e:
        print('raised to the caller', repr(e))
    try:
        next(g)
    except StopIteration:
        print('then over')
    return g


def thrown_twice():
    g = gen(load())
    for _ in range(2):
        try:
            g.throw(ValueError)
        except ValueError:
            pass
    return g


def closed():
    g = gen(load())
    g.close()
    return g


def async_shape(op):
    async def main():
        g = agen(load())
        try:
            await op(g)
        except KeyError as e:
            print('raised to the awaiter', repr(e))
        n = len(load())
        try:
            await g.__anext__()
        except StopAsyncIteration:
            print('then over')
        return g, n

    return asyncio.run(main())


check('throw', thrown)
check('throw twice', thrown_twice)
check('close', closed)
try:
    print('athrow ok', async_shape(lambda g: g.athrow(KeyError('k')))[1])
except MemoryError:
    print('athrow MemoryError: the argument is still held')
try:
    print('aclose ok', async_shape(lambda g: g.aclose())[1])
except MemoryError:
    print('aclose MemoryError: the argument is still held')
