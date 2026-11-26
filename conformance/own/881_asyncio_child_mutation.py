import asyncio

# Shadow and delete exports before the other namespace has read them.
asyncio.Lock = 'package'
from asyncio.locks import Lock
assert Lock is not asyncio.Lock
asyncio.locks.Event = 'child'
assert asyncio.Event is not asyncio.locks.Event
del asyncio.locks.Condition
assert 'Condition' not in dir(asyncio.locks)
assert 'Condition' not in vars(asyncio.locks)
assert asyncio.Condition.__name__ == 'Condition'
asyncio.locks.Condition = asyncio.Condition

asyncio.locks.extra = 2
from asyncio.locks import *
assert 'extra' not in globals()
assert Event == 'child' and Condition is asyncio.Condition

import importlib
import asyncio.locks as original
del asyncio.locks
assert not hasattr(asyncio, 'locks')
assert importlib.import_module('asyncio.locks') is original
assert importlib.import_module('.locks', package='asyncio') is original
from asyncio import locks
assert locks is original and not hasattr(asyncio, 'locks')
