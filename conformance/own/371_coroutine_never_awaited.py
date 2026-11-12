# A coroutine created and never awaited gets CPython's RuntimeWarning on
# stderr. micro-monty reports it when the cell ends, by a scan rather than
# a finalizer (docs/spec.md §1): CPython reports a module-level one at
# shutdown, which is the same moment for a script.
import asyncio

async def fetch(n):
    return n * 2

async def main():
    a = await fetch(1)
    b, c = await asyncio.gather(fetch(2), fetch(3))
    return a + b + c

print(asyncio.run(main()))
pending = fetch(4)
print('done')
