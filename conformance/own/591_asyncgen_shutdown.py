# An async generator first iterated inside `asyncio.run` and not finished
# when the run ends is closed by the run, as CPython's
# `loop.shutdown_asyncgens()` closes it: its `finally` runs, awaits
# included. CPython closes one that became garbage earlier, when it is
# collected; here every one is closed at the end of the run, so the logs
# below are compared sorted, or after the run.
import asyncio
import contextlib
import io


def mk(tag, log):
    async def agen():
        try:
            yield 1
            yield 2
        finally:
            log.append(tag + ' finally')
            await asyncio.sleep(0)
            log.append(tag + ' after its await')
    return agen()


# Kept alive past the run.
log = []
kept = None


async def main1():
    global kept
    kept = mk('kept', log)
    print(await anext(kept))
    log.append('main ends')


asyncio.run(main1())
print(1, log)


async def after1():
    try:
        await anext(kept)
    except StopAsyncIteration:
        return 'closed'


print(1, asyncio.run(after1()), log)

# Several, and ones that are garbage before the run ends.
log = []


async def main2():
    gens = [mk(str(i), log) for i in range(3)]
    for g in gens:
        await anext(g)
    async for x in mk('broken out of', log):
        break
    g = mk('dropped', log)
    await anext(g)
    del g
    # Time for CPython's closes of the two that are garbage.
    for _ in range(4):
        await asyncio.sleep(0)


asyncio.run(main2())
print(2, sorted(log))

# Finished, closed or never started: nothing more runs.
log = []


async def main3():
    a = mk('finished', log)
    async for x in a:
        pass
    b = mk('closed', log)
    await anext(b)
    await b.aclose()
    c = mk('never started', log)
    log.append('main ends')
    return a, b, c


held = asyncio.run(main3())
print(3, log)

# The generator of a task the run cancels at its end.
log = []


async def main4():
    async def worker():
        async for x in mk('worker', log):
            try:
                await asyncio.sleep(10)
            finally:
                log.append('worker cancelled')

    global task
    task = asyncio.create_task(worker())
    await asyncio.sleep(0)
    await asyncio.sleep(0)


asyncio.run(main4())
print(4, log, task.cancelled())

# A close that fails is reported; the others still close.
log = []


async def ignores():
    try:
        yield 1
    finally:
        log.append('ignores finally')
        yield 2


async def raises():
    try:
        yield 1
    finally:
        raise KeyError('in finally')


async def main5():
    global held5
    held5 = [ignores(), raises(), mk('good', log)]
    for g in held5:
        await anext(g)


err = io.StringIO()
with contextlib.redirect_stderr(err):
    asyncio.run(main5())
print(5, sorted(log))
reports = err.getvalue().split('an error occurred during closing of asynchronous generator')[1:]
print(5, sorted(text.strip().splitlines()[-1] for text in reports))

# A Runner closes them when it is closed.
log = []
with asyncio.Runner() as runner:
    async def main6():
        global held6
        held6 = mk('runner', log)
        await anext(held6)

    runner.run(main6())
    log.append('run over')
print(6, log)

# One first stepped while the run's end closes the others (in a `finally`)
# is closed by the run too.
log = []


async def outer7():
    try:
        yield 1
    finally:
        async for x in mk('late', log):
            break
        log.append('outer finally')


async def main7():
    async for x in outer7():
        break


asyncio.run(main7())
print(7, log)
