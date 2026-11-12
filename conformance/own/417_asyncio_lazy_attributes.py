# asyncio's attributes looked up after the module is imported but before
# any of them is used (micro-monty builds them on first use): hasattr,
# identity across lookups and submodules, `from asyncio import *`,
# `__dict__`, assignment before the first lookup and an AttributeError.
import asyncio

x = 0
for i in range(30):
    x += i
print(hasattr(asyncio, 'Lock'), hasattr(asyncio, 'Lok'), x)
L = asyncio.Lock
print(L is asyncio.Lock, L.__name__, L.__module__)
asyncio.Barrier = 5
print(asyncio.Barrier)
names = ['Queue', 'Event', 'Semaphore', 'TaskGroup', 'gather', 'sleep', 'wait_for', 'timeout']
d = asyncio.__dict__
print([n in d for n in names], all(d[n] is getattr(asyncio, n) for n in names))
from asyncio import *
g = globals()
print([g[n] is getattr(asyncio, n) for n in names], 'Lock' in g)
from asyncio import queues, locks
print(queues.Queue is asyncio.Queue, locks.Condition is asyncio.Condition)
try:
    asyncio.Lok
except AttributeError as e:
    print('AttributeError', e)


async def main():
    q = asyncio.Queue()
    await q.put(x)
    async with asyncio.Lock():
        return await asyncio.wait_for(q.get(), 1)


print(asyncio.run(main()))
