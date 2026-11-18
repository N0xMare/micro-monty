# A generator that yields while an incremental mark is in progress saves a
# frame whose registers the last root-taking step did not clear: here the
# list `load()` made, dead once `len` read it. Each shape makes the large
# block twice; the second fits only if the suspended generator's saved copy
# of the first is cleared by a later collection. python3.14 prints "ok" for
# each. `delegating` runs first: run after the 100 MB of the first shape
# are dropped, its allocation would wait for the incremental mark that
# holds them, a MemoryError retry whose charge depends on the GC mode and
# on snapshots, as on 0.0.2 (`RETRY_CHARGE`).

import asyncio
import gc


def load(k, n):
    return [bytearray(n) for _ in range(k)]


def marked_across_the_yield():
    # About 100 MB live, so a mark takes many slices; the mark the first
    # load makes due is still running at the yield.
    keep = [(i, i) for i in range(2_500_000)]

    def gen():
        n = len(load(95, 1_000_000))
        i = 0
        while i < 1000:
            i += 1
        yield n
        yield 0

    g = gen()
    a = next(g)
    i = 0
    while i < 100_000:
        i += 1
    print('ok', a, len(load(95, 1_000_000)), len(keep))


def delegating():
    # The outer generator suspends inside `yield from`, its frame saved
    # with the dead list.
    def inner():
        yield 1
        yield 2

    def outer():
        n = len(load(3, 50_000_000))
        yield from inner()
        yield n

    g = outer()
    a = next(g)
    print('ok', a, len(load(3, 50_000_000)))


def yielded_again():
    # Cleared by a collection, resumed, and suspended again with a new dead
    # list: the yield takes back the cleared mark, or no later collection
    # clears the second copy.
    def gen():
        yield 0
        n = len(load(3, 50_000_000))
        yield n
        yield 0

    async def agen():
        yield 0
        n = len(load(3, 50_000_000))
        yield n
        yield 0

    async def drive():
        ag = agen()
        await anext(ag)
        gc.collect()
        b = await anext(ag)
        print('ok', b, len(load(3, 50_000_000)))

    g = gen()
    next(g)
    gc.collect()
    a = next(g)
    print('ok', a, len(load(3, 50_000_000)))
    asyncio.run(drive())


delegating()
yielded_again()
marked_across_the_yield()
