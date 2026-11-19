# Loop callbacks as the scheduler starts them: how each kind of callable is
# called, what a wrong call reports, and the order of a mix of handles,
# timers and done callbacks registered in one iteration.
import asyncio
import contextlib
import functools
import io

log = []


def plain(*a):
    log.append(('plain', a))


def defaults(a, b=2, *, c=3):
    log.append(('defaults', a, b, c))


def star(first, *rest):
    log.append(('star', first, rest))


def two(a, b):
    log.append(('two', a, b))


class Obj:
    def __init__(self, name):
        self.name = name

    def method(self, *a):
        log.append(('method', self.name, a))

    def __call__(self, *a):
        log.append(('call', self.name, a))


def gen(*a):
    log.append(('gen body', a))
    yield 1


def native_target():
    return []


async def calls():
    loop = asyncio.get_running_loop()
    loop.call_soon(plain)
    loop.call_soon(plain, 1, 'x')
    loop.call_soon(defaults, 1)
    loop.call_soon(defaults, 1, 5)
    loop.call_soon(star, 1)
    loop.call_soon(star, 1, 2, 3)
    loop.call_soon(two, 1)
    loop.call_soon(two, 1, 2, 3)
    loop.call_soon(Obj('m').method, 7)
    loop.call_soon(Obj('c'), 8)
    loop.call_soon(functools.partial(two, 'p'), 'q')
    loop.call_soon(functools.partial(two, 'p'))
    loop.call_soon(gen, 'g')
    loop.call_soon(native_target().append, 'n')
    loop.call_soon(len)
    fut = loop.create_future()
    fut.add_done_callback(plain)
    fut.add_done_callback(Obj('d').method)
    fut.add_done_callback(two)
    fut.set_result('r')
    err = io.StringIO()
    with contextlib.redirect_stderr(err):
        for _ in range(3):
            await asyncio.sleep(0)
    # A wrong call is the callback's error, reported by the loop.
    for text in err.getvalue().split('Exception in callback ')[1:]:
        lines = text.strip().splitlines()
        print('reported:', lines[-1])
    for entry in log:
        if entry[0] in ('plain', 'method') and entry[1:] and isinstance(entry[-1], tuple):
            print(entry[0], [type(x).__name__ for x in entry[-1]])
        else:
            print(entry)
    log.clear()


async def order():
    loop = asyncio.get_running_loop()
    hs = {}

    def note(name):
        log.append(name)

    def cancel_later(name):
        log.append(name)
        hs['victim'].cancel()

    fut = loop.create_future()
    loop.call_soon(note, 'soon 1')
    hs['early'] = loop.call_soon(note, 'cancelled before the loop runs')
    loop.call_later(0, note, 'later 0')
    loop.call_at(loop.time(), note, 'at now')
    fut.add_done_callback(lambda f: note('done ' + f.result()))
    loop.call_soon(cancel_later, 'soon 2 cancels the victim')
    hs['victim'] = loop.call_soon(note, 'victim')
    loop.call_later(0, note, 'later 0 again')
    loop.call_soon(fut.set_result, 'r')
    loop.call_soon(note, 'soon 3')
    hs['early'].cancel()
    t = loop.call_later(0, note, 'timer cancelled')
    t.cancel()
    loop.call_later(0.01, note, 'later 0.01')
    loop.call_soon(loop.call_soon, note, 'soon from a callback')
    for _ in range(4):
        await asyncio.sleep(0)
    log.append('main after four iterations')
    await asyncio.sleep(0.02)
    for name in log:
        print(name)
    print(hs['early'], hs['victim'], t.cancelled())


asyncio.run(calls())
asyncio.run(order())
