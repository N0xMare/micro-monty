# A register a frame waits with holds nothing for a collection unless the
# frame stored to it and reads it again: in the saved frame of a coroutine
# suspended by `send` or of a task, in the targets of an unpack while a Python
# iterator runs, in a method call's callee while a descriptor's code runs,
# and in a register an instruction names but does not read. Each shape
# makes 150 MB twice under the 256 MiB limit (in 50 MB blocks, under the
# object size cap) and binds the first to no name; the second fits only if
# the first is not a root while the second is made. python3.14 prints "ok"
# for each.

N = 50_000_000


def load():
    return [bytearray(N) for _ in range(3)]


class It:
    # An iterator whose first item makes the second 150 MB.
    def __init__(self, n):
        self.k = 0
        self.n = n

    def __iter__(self):
        return self

    def __next__(self):
        self.k += 1
        if self.k == 1:
            return len(load())
        if self.k <= self.n:
            return self.k
        raise StopIteration


class Step:
    def __await__(self):
        yield 1


def coroutine_send():
    # A coroutine driven by `send`, saved at its `await`.
    async def co():
        n = len(load())
        await Step()
        return n

    c = co()
    c.send(None)
    m = len(load())
    try:
        c.send(None)
    except StopIteration as e:
        return e.value, m


def task_await():
    # A task waiting on another: its frames stay on the task's stack.
    import asyncio

    async def first():
        n = len(load())
        await asyncio.sleep(0)
        return n

    async def second():
        return len(load())

    async def main():
        return await asyncio.gather(first(), second())

    return asyncio.run(main())


def unpack():
    it = It(3)
    n = len(load())
    [a, b, c] = it
    return a + b + c + n


def starred():
    it = It(4)
    n = len(load())
    a, b, *c = it
    return a + b + sum(c) + n


def nested():
    it = It(2)
    (0, 0, 0, load())
    x, (a, b) = 1, it
    return x + a + b


def for_targets():
    t = 0
    (0, 0, load())
    for a, b in [It(2)]:
        t += a + b
    return t


class Ctx:
    def __enter__(self):
        return It(2)

    def __exit__(self, *exc):
        return False


def with_targets():
    n = len(load())
    with Ctx() as (a, b):
        return a + b + n


class Desc:
    # A descriptor whose `__get__` makes the second 150 MB.
    def __get__(self, obj, cls):
        n = len(load())
        return lambda: n


class C:
    m = Desc()

    @property
    def p(self):
        n = len(load())
        return lambda: n


class Fallback:
    def __getattr__(self, name):
        n = len(load())
        return lambda: n


def descriptor():
    c = C()
    [load(), 0]
    return c.m()


def descriptor_on_class():
    [load(), 0]
    return C.m()


def prop():
    c = C()
    [load(), 0]
    return c.p()


def getattr_fallback():
    f = Fallback()
    [load(), 0]
    return f.m()


def unpack_raises():
    # The targets an unpack fills are its own: a failed one leaves the
    # names as they were.
    a, b = 1, 2

    class Bad:
        def __iter__(self):
            return self

        def __next__(self):
            raise ValueError('bad')

    try:
        a, b = Bad()
    except ValueError:
        pass
    return a, b


SHAPES = [
    coroutine_send, task_await, unpack, starred, nested, for_targets,
    with_targets, descriptor, descriptor_on_class, prop, getattr_fallback,
    unpack_raises,
]
for shape in SHAPES:
    try:
        print(shape.__name__, 'ok', shape())
    except MemoryError:
        print(shape.__name__, 'MemoryError: the first 150 MB is still held')


def g(**kw):
    return len(kw)


# At module level, where register 0 is a temporary: an instruction mode
# that reads no register names register 0 (`DICT_UPDATE` of a call's `**`
# and of a display's `**`).
try:
    [load(), 0]
    x = [g(**{}, k=len(load())), 1]
    print('call spread ok', x)
except MemoryError:
    print('call spread MemoryError: the first 150 MB is still held')
try:
    [load(), 0]
    x = [{**{}, 'k': len(load())}, 1]
    print('display spread ok', x)
except MemoryError:
    print('display spread MemoryError: the first 150 MB is still held')
