# Generator methods as __iter__, pipelines, iter(callable, sentinel), states.
class Tree:
    def __init__(self, value, *children):
        self.value = value
        self.children = children

    def __iter__(self):
        yield self.value
        for c in self.children:
            yield from c

    def walk(self, depth=0):
        yield depth, self.value
        for c in self.children:
            yield from c.walk(depth + 1)


t = Tree(1, Tree(2, Tree(4)), Tree(3))
print(list(t), sum(t), max(t), sorted(t, reverse=True), 4 in t, list(t.walk()))


def numbers():
    n = 0
    while True:
        yield n
        n += 1


def take(n, it):
    for i, x in enumerate(it):
        if i >= n:
            return
        yield x


def evens(it):
    return (x for x in it if x % 2 == 0)


def squares(it):
    for x in it:
        yield x * x


print(list(take(5, squares(evens(numbers())))))


class Source:
    def __init__(self, items):
        self.items = list(items)

    def read(self):
        return self.items.pop(0) if self.items else ''


src = Source(['a', 'b', '', 'c'])
print(list(iter(src.read, '')), src.items)
calls = []


def tick():
    calls.append(1)
    return len(calls)


it = iter(tick, 4)
print(type(it).__name__, next(it), list(it), next(it, 'done'), calls)


def state():
    yield 1


g = state()
print(g.gi_running, next(g), next(g, 'end'))


def running():
    yield gen.gi_running


gen = running()
print(next(gen))


def echo():
    received = []
    while True:
        try:
            x = yield len(received)
        except GeneratorExit:
            received.append('exit')
            print('echo closing with', received)
            raise
        received.append(x)


e = echo()
next(e)
e.send('a')
e.send('b')
e.close()


def gen_with_return_in_finally():
    try:
        yield 1
    finally:
        return 'from finally'


g = gen_with_return_in_finally()
next(g)
try:
    g.throw(ValueError('swallowed by return'))
except StopIteration as e:
    print('StopIteration', e.value)


def chain(*its):
    for it in its:
        yield from it


print(list(chain('ab', [1], (x for x in range(2)), {'k': 0})))
z = zip(numbers(), 'xyz')
print(list(z), list(z))
m = map(lambda a, b: a + b, numbers(), [10, 20, 30])
print(list(m))
print(list(filter(lambda x: x % 3 == 0, take(10, numbers()))))
print(dict(enumerate(take(3, numbers()))), list(zip(*[(1, 'a'), (2, 'b')])))


class Countdown:
    def __init__(self, n):
        self.n = n

    def __iter__(self):
        n = self.n
        while n > 0:
            yield n
            n -= 1


cd = Countdown(3)
print(list(cd), list(cd), [a * b for a in cd for b in cd if a > b])
