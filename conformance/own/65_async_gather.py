# asyncio.run, await of local coroutines, gather ordering and interleaving.
import asyncio
import sys


async def square(x):
    return x * x


async def chain(n):
    total = 0
    for i in range(n):
        total += await square(i)
    return total


async def worker(name, steps, log):
    for i in range(steps):
        log.append(f'{name}{i}')
        await asyncio.sleep(0)
    return name.upper()


async def delayed(delay, tag, log):
    await asyncio.sleep(delay)
    log.append(tag)
    return tag


def make_adder(k):
    async def add(x):
        return x + k

    return add


async def main():
    print(await square(7), await chain(5))
    log = []
    results = await asyncio.gather(worker('a', 3, log), worker('b', 1, log), worker('c', 2, log))
    print(results, log)
    timed = []
    print(await asyncio.gather(delayed(0.03, 'slow', timed), delayed(0.01, 'fast', timed), delayed(0.02, 'mid', timed)))
    print(timed)
    print(await asyncio.gather())
    print(await asyncio.sleep(0, 'slept'), await asyncio.sleep(0.001, result=[1, 2]))
    add5 = make_adder(5)
    print(await asyncio.gather(*(add5(i) for i in range(4))))
    nested = await asyncio.gather(asyncio.gather(square(2), square(3)), chain(3))
    print(nested)
    print('to stderr', file=sys.stderr)
    sys.stdout.write('written\n')
    return 'done'


print(asyncio.run(main()))
print(asyncio.run(chain(4)))


# Awaitables shared between gathers, duplicates, failures settled before and
# after registration, and a gather wide enough to expose per-task rescans.
async def one(i):
    await asyncio.sleep(0)
    return i


async def bad(d):
    await asyncio.sleep(d)
    raise ValueError(d)


async def shared():
    g0 = asyncio.gather(one(5))
    g1 = asyncio.gather(g0, g0, one(1))
    g2 = asyncio.gather(g0, g1, g0)
    print(await g2, await g1, await g0)
    try:
        await asyncio.gather(one(1), bad(0.2), bad(0.1))
    except ValueError as e:
        print('first failure', e)
    print(await asyncio.gather(one(1), bad(0), return_exceptions=True))
    done = asyncio.gather(one(3))
    await done
    print(await asyncio.gather(done, one(4), done))
    wide = await asyncio.gather(*[one(i) for i in range(3000)])
    print(len(wide), sum(wide), wide[:3], wide[-1])


asyncio.run(shared())
