# Generators with try/finally/except under send/throw/close sequences.
def g1():
    try:
        x = yield 'first'
        print('got', x)
        try:
            yield 'inner'
        finally:
            print('inner finally')
        yield 'after inner'
    except ZeroDivisionError:
        print('caught ZDE')
        yield 'zde-handled'
    finally:
        print('outer finally')


g = g1()
print(next(g), g.send(1), g.throw(ZeroDivisionError))
print(list(g))

g = g1()
next(g)
g.send(None)
print(g.throw(ZeroDivisionError()))
g.close()
print('---')


def g2():
    for i in range(3):
        try:
            yield i
        except ValueError:
            print('skip', i)


g = g2()
print(next(g), g.throw(ValueError), g.throw(ValueError))
try:
    g.throw(ValueError)
except StopIteration:
    print('exhausted after throws')
except ValueError:
    print('ValueError escaped')


def g3():
    yield 1
    try:
        yield 2
    except GeneratorExit:
        print('GeneratorExit seen')
        return 'ignored return'


g = g3()
next(g)
g.close()
g = g3()
next(g)
next(g)
print(g.close())


def g4():
    try:
        yield
    except Exception as e:
        print('g4 caught', type(e).__name__)
        raise TypeError('converted') from e


g = g4()
next(g)
try:
    g.throw(KeyError('k'))
except TypeError as e:
    print(repr(e), repr(e.__cause__))


def g5():
    try:
        yield 1
    finally:
        yield 'in finally'


g = g5()
next(g)
print(g.throw(ValueError('v')) if False else next(g))
try:
    next(g)
except StopIteration:
    print('done g5')


def g6():
    x = 0
    while True:
        try:
            x = yield x
        except OverflowError as e:
            x = str(e)


g = g6()
next(g)
print(g.send(5), g.throw(OverflowError('reset')), g.send(7))
g.close()
print(next(g, 'closed'))


def counter():
    i = 0
    try:
        while True:
            i += 1
            yield i
    finally:
        print('counter finally at', i)


c = counter()
for v in c:
    if v == 3:
        break
c.close()
print('closed counter', next(c, None))
