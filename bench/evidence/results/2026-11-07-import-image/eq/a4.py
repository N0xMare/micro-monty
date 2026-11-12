import asyncio
lk = asyncio.Lock()
#---
from asyncio import Queue, Event
import asyncio.queues
#---
asyncio.Queue is Queue, asyncio.queues.Queue is Queue, lk.locked()
