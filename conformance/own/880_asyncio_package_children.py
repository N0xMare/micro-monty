import asyncio

# A durability run can park after import with the child exports untouched.
for _ in range(20):
    pass

names = ('events', 'exceptions', 'futures', 'locks', 'queues', 'runners',
         'taskgroups', 'tasks', 'timeouts')
visible = dir(asyncio)
assert all(name in visible for name in names)

import sys

registry = sys.modules
for name in names:
    child = getattr(asyncio, name)
    dotted = 'asyncio.' + name
    assert child.__name__ == dotted
    assert registry[dotted] is child
    assert dotted in registry

import fractions

assert registry.get('fractions') is fractions
assert '_mm' not in registry and '_mm' not in registry.keys()

assert 'Lock' in dir(asyncio.locks)
from asyncio.locks import Lock
assert Lock is asyncio.Lock
assert asyncio.Queue is asyncio.queues.Queue
assert asyncio.exceptions.CancelledError is asyncio.CancelledError
assert asyncio.tasks.gather is asyncio.gather
assert vars(asyncio.runners)['run'] is asyncio.run
namespace = vars(asyncio)
assert all(namespace[name] is getattr(asyncio, name) for name in names)

asyncio.extra = 1
from asyncio import *
assert 'queues' not in globals() and 'extra' not in globals()
assert gather is asyncio.tasks.gather
