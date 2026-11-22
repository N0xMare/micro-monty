# `add_done_callback` takes any object, as CPython's does: one that is not
# callable is called when the future is done and the loop reports its
# TypeError. A gather passed as the callback is not one of the future's
# children: it still waits for its own.
import asyncio
import contextlib
import io


def reports(err):
    out = []
    for text in err.getvalue().split('Exception in callback ')[1:]:
        # micro-monty appends a hint to the report of an awaitable.
        out.append(text.strip().splitlines()[-1].split('. Did you forget')[0])
    return sorted(out)


async def main():
    loop = asyncio.get_running_loop()
    a, b = loop.create_future(), loop.create_future()
    g = asyncio.gather(a, b)
    f1, f2 = loop.create_future(), loop.create_future()
    f1.add_done_callback(g)
    f2.add_done_callback(g)
    err = io.StringIO()
    with contextlib.redirect_stderr(err):
        f1.set_result(1)
        f2.set_result(2)
        for _ in range(3):
            await asyncio.sleep(0)
    print('gather done:', g.done(), reports(err))
    a.set_result('a')
    b.set_result('b')
    print(await g)

    f3 = loop.create_future()
    ran = []
    f3.add_done_callback(lambda f: ran.append('first'))
    for cb in [1, 0, True, None, 'x', 2 ** 70, g, asyncio.gather(loop.create_future())]:
        f3.add_done_callback(cb)
    f3.add_done_callback(lambda f: ran.append('last'))
    t = asyncio.create_task(asyncio.sleep(0, 'slept'))
    t.add_done_callback(asyncio.gather(loop.create_future()))
    t.add_done_callback(None)
    done = loop.create_future()
    done.set_result(0)
    done.add_done_callback(7)
    err = io.StringIO()
    with contextlib.redirect_stderr(err):
        f3.set_result(0)
        print(await t)
        for _ in range(3):
            await asyncio.sleep(0)
    print(ran)
    for line in reports(err):
        print(line)
    # Removing one finds nothing more to remove afterwards.
    f4 = loop.create_future()
    f4.add_done_callback(print)
    print(f4.remove_done_callback(print), f4.remove_done_callback(print))


asyncio.run(main())
