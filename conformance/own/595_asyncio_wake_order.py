# The order blocked tasks resume in, when tasks before them in the loop's
# list of blocked tasks have woken or been cancelled and others have
# blocked since: sleeps due together (in the order they began), sleeps
# due at different times, tasks waiting on futures settled out of order,
# a sleeping task cancelled from the middle, and the main task blocked
# among them.
import asyncio

log = []


async def sleeper(name, delay):
    try:
        await asyncio.sleep(delay)
        log.append(name)
    except asyncio.CancelledError:
        log.append(name + ' cancelled')


async def waiter(name, f):
    try:
        log.append(f'{name}={await f}')
    except asyncio.CancelledError:
        log.append(name + ' cancelled')


def show(title):
    print(title, ' '.join(log))
    log.clear()


async def main():
    loop = asyncio.get_running_loop()
    fs = [loop.create_future() for _ in range(48)]
    ws = []
    first = []
    for i, f in enumerate(fs):
        ws.append(asyncio.create_task(waiter(f'w{i}', f)))
        if i % 4 == 0:
            first.append(asyncio.create_task(sleeper(f'a{i}', 0.5)))
        if i % 6 == 1:
            first.append(asyncio.create_task(sleeper(f'b{i}', 0.03)))
    await asyncio.sleep(0)
    # 48 waiters and 20 sleepers are blocked, in the order they began.
    for i in (30, 2, 47, 11, 0, 19, 40):
        fs[i].set_result(i)
    ws[5].cancel()
    first[3].cancel()
    await asyncio.sleep(0)
    show('settled out of order:')
    # Most of the waiters wake, from both ends and the middle.
    for i in list(range(46, 20, -1)) + list(range(1, 20, 2)) + [20, 4, 6]:
        if not fs[i].done():
            fs[i].set_result(-i)
    first[8].cancel()
    await asyncio.sleep(0)
    show('most woke:')
    # More tasks block behind what is left, due with the first ones.
    later = [asyncio.create_task(sleeper(f'c{i}', 0.5)) for i in range(9)]
    extra = [loop.create_future() for _ in range(5)]
    held = [asyncio.create_task(waiter(f'x{i}', f)) for i, f in enumerate(extra)]
    await asyncio.sleep(0)
    later[4].cancel()
    extra[3].set_result('three')
    first[10].cancel()
    # The main task blocks too, and wakes between the two sets of sleeps.
    await asyncio.sleep(0.1)
    show('after the short sleeps:')
    extra[0].set_result('zero')
    for i in (8, 10, 12):
        fs[i].set_result(i * 100)
    await asyncio.sleep(0.6)
    show('after the long sleeps:')
    # Again, with every sleep due together and the list mostly empty.
    again = [asyncio.create_task(sleeper(f'd{i}', 0.02)) for i in range(40)]
    await asyncio.sleep(0)
    for i in range(0, 40, 3):
        again[i].cancel()
    for i in (37, 5, 22):
        again[i].cancel()
    tail = [asyncio.create_task(sleeper(f'e{i}', 0.02)) for i in range(6)]
    await asyncio.sleep(0.04)
    show('due together:')
    for f in fs + extra:
        if not f.done():
            f.cancel()
    await asyncio.gather(*ws, *held, *first, *later, *again, *tail)
    show('the rest:')


asyncio.run(main())
