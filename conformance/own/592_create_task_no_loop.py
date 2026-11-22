# `asyncio.create_task` outside a running loop raises CPython's
# RuntimeError (micro-monty's text goes on after a `;` with what to do
# instead, so the text is compared up to there); `loop.create_task` on a
# loop that is not running takes the task, which runs when the loop does.
import asyncio


async def co(n=1):
    return n


c = co()
for make in (asyncio.create_task, lambda c: asyncio.create_task(c, name='n')):
    try:
        make(c)
        print('accepted')
    except RuntimeError as e:
        print('RuntimeError', str(e).split(';')[0])
# The loop is checked before the argument.
try:
    asyncio.create_task(1)
except (RuntimeError, TypeError) as e:
    print(type(e).__name__, str(e).split(';')[0])
c.close()

loop = asyncio.new_event_loop()
t = loop.create_task(co(4), name='x')
print(t.get_name(), t.done(), loop.run_until_complete(t))
loop.close()


async def main():
    import contextvars
    a = asyncio.create_task(co(1), name='n')
    b = asyncio.get_running_loop().create_task(co(2))
    c = asyncio.ensure_future(co(3))
    async with asyncio.TaskGroup() as tg:
        d = tg.create_task(co(5))
    e = asyncio.create_task(co(6), context=contextvars.copy_context())
    f = asyncio.create_task(co(7), name=8)
    print(a.get_name(), f.get_name(), await asyncio.gather(a, b, c, d, e, f))


asyncio.run(main())
with asyncio.Runner() as r:
    print(r.run(co(9)))
    t = r.get_loop().create_task(co(10))
    print(r.run(co(11)), t.done(), t.result())
