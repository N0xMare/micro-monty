# A walrus in a generator expression binds in the scope around it, as in a
# list, set or dict comprehension: the module's globals, a function's local
# (a cell the generator writes as it runs), or a `global` or `nonlocal`
# name of that function.
hit = 0
print(any((hit := x) < 0 for x in [1, -2]), hit)
print(list((m := i) for i in range(3)), m)


def f():
    g = (z := x for x in [0, 5])
    print(list(g), z)


f()


def nested():
    r = [list((w := (a, b)) for b in range(2)) for a in range(2)]
    print(r, w)
    s = list(((q := c) for c in range(2)) for _ in range(1))
    print([list(x) for x in s], q)
    t = {(k := j): j for j in range(2)}
    print(t, k)
    u = {(v := j) for j in range(3)}
    print(u, v)

    def inner():
        return sum((y := n) for n in range(4)), y

    print(inner())


nested()


def with_global():
    global gg
    list((gg := n) for n in range(2))


with_global()
print(gg)


def with_nonlocal():
    nn = 0

    def inner():
        nonlocal nn
        list((nn := n) for n in range(7))

    inner()
    print(nn)


with_nonlocal()


# The binding happens as the generator runs, and closures see it.
def lazily():
    gen = ((late := i) for i in range(3))
    try:
        print(late)
    except NameError as e:
        print(type(e).__name__, e)
    next(gen)
    print(late)
    next(gen)

    def look():
        return late

    print(look())
    list(gen)
    print(late, look())


lazily()


class K:
    def m(self):
        return list((y := 1) for x in [1]), y


print(K().m())
print((lambda: list((lw := i) for i in range(2)) and lw)())
# A lambda inside the generator is a scope of its own.
print(list((lambda: (q := 1)) for x in [1])[0]())

import asyncio


async def agen():
    async def src():
        for i in range(3):
            yield i

    g = ((al := x) async for x in src())
    print([x async for x in g], al)


asyncio.run(agen())
