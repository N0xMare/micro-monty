# Generator protocol: send, throw, close, return values, states.
def counter(n):
    print('start')
    total = 0
    for i in range(n):
        got = yield i
        if got:
            total += got
    return total


g = counter(3)
print(type(g).__name__, g.__name__, g.__qualname__)
print(next(g), g.send(10), g.send(5))
try:
    g.send(1)
except StopIteration as e:
    print('StopIteration', e.value, e.args)
print(next(g, 'exhausted'), list(g))

g = counter(2)
try:
    g.send(1)
except TypeError as e:
    print(e)
print(g.send(None))


def guarded():
    try:
        yield 1
        yield 2
    except ValueError as e:
        print('caught', e)
        yield 'recovered'
    finally:
        print('cleanup')


g = guarded()
print(next(g), g.throw(ValueError('boom')))
try:
    next(g)
except StopIteration:
    print('done')

g = guarded()
next(g)
g.close()
print('closed', next(g, 'none'))
g.close()

g = guarded()
try:
    g.throw(KeyError('early'))
except KeyError as e:
    print('propagated', e)


def stubborn():
    try:
        yield 1
    except GeneratorExit:
        yield 'refuse'


g = stubborn()
next(g)
try:
    g.close()
except RuntimeError as e:
    print(e)


def returner():
    yield 1
    return 'result'


g = returner()
print(list(g), next(g, 'default'))


def raiser():
    yield 1
    raise StopIteration('bad')


try:
    list(raiser())
except RuntimeError as e:
    print(repr(e), repr(e.__cause__))


def selfref():
    yield g2.send(None)


g2 = selfref()
try:
    next(g2)
except ValueError as e:
    print(e)

g = counter(1)
print(g.throw(ValueError, 'x') if False else 'skip')
try:
    g.throw(ValueError('before start'))
except ValueError as e:
    print('before start:', e)
print(next(g, 'finished'))


def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b


f = fib()
print([next(f) for _ in range(10)])


def gen_exc_tb():
    yield 1
    1 / 0


for x in gen_exc_tb():
    print(x)
