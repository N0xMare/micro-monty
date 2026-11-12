# yield from: delegation, return values, send/throw/close passthrough.
def inner():
    try:
        x = yield 'i1'
        print('inner got', x)
        y = yield 'i2'
        print('inner got', y)
    except KeyError as e:
        print('inner caught', repr(e))
        yield 'i-recovered'
    finally:
        print('inner finally')
    return 'inner-result'


def outer():
    r = yield from inner()
    print('outer received', r)
    yield 'o-last'
    return 'outer-result'


g = outer()
print(next(g), g.send('A'), g.send('B'))
try:
    next(g)
except StopIteration as e:
    print('stop', e.value)

g = outer()
next(g)
print(g.throw(KeyError('k')))
print(next(g))
g.close()

g = outer()
next(g)
g.close()
print('closed outer')


def lists():
    yield from [1, 2]
    yield from (x * 10 for x in range(2))
    yield from 'ab'
    yield from {'k': 1}
    r = yield from range(0)
    print('range result', r)


print(list(lists()))


class It:
    def __init__(self):
        self.n = 0

    def __iter__(self):
        return self

    def __next__(self):
        self.n += 1
        if self.n > 2:
            raise StopIteration
        return self.n


def from_user():
    yield from It()


print(list(from_user()))


def delegate_send():
    yield from It()


g = delegate_send()
next(g)
try:
    g.send('x')
except AttributeError as e:
    print('AttributeError', e)


def thrower():
    yield from [1, 2, 3]


g = thrower()
next(g)
try:
    g.throw(ValueError('into list'))
except ValueError as e:
    print('ValueError', e)


def depth(n):
    if n == 0:
        yield 'bottom'
        return 0
    got = yield from depth(n - 1)
    yield f'level {n}'
    return got + 1


g = depth(3)
print(list(g))


def ret_value():
    return (yield from depth(2))


g = ret_value()
for v in g:
    pass
try:
    next(g)
except StopIteration as e:
    print('after', e.value)


def accumulate():
    total = 0
    while True:
        x = yield total
        if x is None:
            return total
        total += x


def wrapper():
    result = yield from accumulate()
    yield f'final {result}'


g = wrapper()
next(g)
for n in (1, 2, 3):
    print(g.send(n))
print(g.send(None))


def not_iterable():
    yield from 5


try:
    list(not_iterable())
except TypeError as e:
    print(e)


def gen_with_close_error():
    try:
        yield 1
    finally:
        raise ValueError('in finally')


def wrapper2():
    yield from gen_with_close_error()


g = wrapper2()
next(g)
try:
    g.close()
except ValueError as e:
    print('close raised', e)
