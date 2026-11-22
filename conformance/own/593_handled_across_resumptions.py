# The exception a generator or coroutine handles, across resumptions. A
# handler that began under one resumption and ends under another gives back
# the handled exception of the caller that resumed it now, not of the one
# that resumed it then: `sys.exception()`, a bare `raise` and the
# `__context__` of what is raised next read as on CPython. A generator that
# handles the very exception its caller handles keeps it while its handler
# lasts.
import sys


# --- part 1: one generator, one coroutine, one async generator
def gen():
    try:
        raise KeyError('k')
    except KeyError:
        yield 1
    print('after handler', repr(sys.exception()))
    try:
        raise
    except RuntimeError as e:
        print('bare raise', repr(e))
    except BaseException as e:
        print('bare raise re-raised', repr(e))
    try:
        raise OSError('o')
    except OSError as e:
        print('context', repr(e.__context__))
    yield 2

g = gen()
try:
    raise ValueError('a')
except ValueError:
    next(g)
print(next(g))
print('module', repr(sys.exception()))

# same object thrown in and handled by both
def gen2():
    try:
        yield 0
    except ValueError:
        yield 1
        print('still in handler', repr(sys.exception()))
    yield 2
g = gen2()
next(g)
try:
    raise ValueError('same')
except ValueError as e:
    g.throw(e)
next(g)

# nested handlers
def gen3():
    try:
        raise KeyError('outer')
    except KeyError:
        try:
            raise IndexError('inner')
        except IndexError:
            yield 1
        print('after inner', repr(sys.exception()))
        yield 2
    print('after outer', repr(sys.exception()))
    yield 3
g = gen3()
try:
    raise ValueError('c1')
except ValueError:
    next(g)
try:
    raise TypeError('c2')
except TypeError:
    next(g)
    print(next(g))

async def agen():
    try:
        raise KeyError('ak')
    except KeyError:
        yield 1
    print('agen after handler', repr(sys.exception()))
    yield 2

async def main():
    it = agen()
    try:
        raise ValueError('am')
    except ValueError:
        await anext(it)
    await anext(it)
    print('main', repr(sys.exception()))

import asyncio
asyncio.run(main())

async def co():
    try:
        raise KeyError('ck')
    except KeyError:
        await Y()
    print('coro after handler', repr(sys.exception()))

class Y:
    def __await__(self):
        yield 1
c = co()
try:
    raise ValueError('cm')
except ValueError:
    c.send(None)
try:
    c.send(None)
except StopIteration:
    pass


# --- part 2: chains, throw and close, finally, with, except*
def see(tag):
    out = [tag, repr(sys.exception())]
    try:
        raise
    except RuntimeError as e:
        out.append('none active' if 'No active' in str(e) else 're ' + repr(e))
    except BaseException as e:
        out.append('re ' + repr(e))
    try:
        raise OSError('probe')
    except OSError as e:
        out.append(repr(e.__context__))
    print(*out)


class Aw:
    def __await__(self):
        return (yield 'aw')


def under(exc, f, *a):
    if exc is None:
        return f(*a)
    try:
        raise exc
    except type(exc):
        return f(*a)


def drive(c, plan):
    # plan: exceptions (or None) the caller handles at each send
    for exc in plan:
        try:
            print('yielded', under(exc, c.send, None))
        except StopIteration as s:
            print('returned', s.value)
            break
        except BaseException as e:
            print('raised', repr(e), repr(e.__context__))
            break
        see('caller')


# 1. chain: inner in a handler, outer not
async def inner1():
    try:
        raise KeyError('i1')
    except KeyError:
        see('inner in handler a')
        await Aw()
        see('inner in handler b')
    see('inner after')
    await Aw()
    see('inner after 2')
    return 'i'


async def outer1():
    see('outer start')
    r = await inner1()
    see('outer end')
    return r

drive(outer1(), [ValueError('c1'), None, TypeError('c3'), None])

# 2. chain: both in handlers
async def outer2():
    try:
        raise IndexError('o2')
    except IndexError:
        await inner1()
        see('outer in handler')
    see('outer after')
    await Aw()
    see('outer after 2')

drive(outer2(), [ValueError('d1'), TypeError('d2'), None, LookupError('d4'), None])

# 3. finally under an exception with a suspension
async def fin():
    try:
        try:
            raise KeyError('f')
        finally:
            await Aw()
            see('in finally')
    except KeyError:
        await Aw()
        see('in except')
    see('fin after')

drive(fin(), [ValueError('e1'), None, TypeError('e3'), None])

# 4. generator: return inside a handler, raise out of a handler
def g_ret():
    try:
        raise KeyError('g')
    except KeyError:
        yield 1
        return 5

def g_raise():
    try:
        raise KeyError('g')
    except KeyError:
        yield 1
        raise IndexError('out')

for mk in g_ret, g_raise:
    g = mk()
    under(ValueError('h1'), next, g)
    try:
        under(TypeError('h2'), next, g)
    except StopIteration as s:
        print('stop', s.value)
    except IndexError as e:
        print('raised', repr(e), repr(e.__context__), repr(e.__context__.__context__))
    see('after ' + mk.__name__)

# 5. yield from: delegating generator in a handler, inner in a handler
def leaf():
    try:
        raise KeyError('leaf')
    except KeyError:
        yield 'l1'
        see('leaf in handler')
    see('leaf after')
    yield 'l2'
    see('leaf after 2')

def mid():
    try:
        raise IndexError('mid')
    except IndexError:
        yield from leaf()
        see('mid in handler')
    see('mid after')
    yield 'm'

g = mid()
print(under(ValueError('y1'), next, g))
print(under(None, next, g))
print(under(TypeError('y3'), next, g))
print(list(g))

def mid2():
    yield from leaf()
    see('mid2 after')
g = mid2()
print(under(ValueError('z1'), next, g))
print(under(None, next, g))
print(under(TypeError('z3'), next, g, 'end'))

# 6. throw and close into a generator inside its handler
def g_tc():
    try:
        raise KeyError('tc')
    except KeyError:
        try:
            yield 1
        except ZeroDivisionError as e:
            print('caught', repr(e), repr(e.__context__))
            see('after catch, still in handler')
        finally:
            see('finally')
    see('g_tc after')
    yield 2

g = g_tc()
under(ValueError('t1'), next, g)
print(under(TypeError('t2'), g.throw, ZeroDivisionError('z')))
see('module')
g = g_tc()
under(ValueError('t1'), next, g)
under(TypeError('t3'), g.close)
see('module 2')

# 7. with statement exit inside a generator
class CM:
    def __enter__(self):
        return self
    def __exit__(self, *a):
        see('exit')
        return True

def g_with():
    with CM():
        try:
            raise KeyError('w')
        finally:
            yield 1
    see('g_with after')
    yield 2

g = g_with()
under(ValueError('w1'), next, g)
under(None, next, g)

# 8. except*
def g_star():
    try:
        raise ExceptionGroup('eg', [KeyError('a'), IndexError('b')])
    except* KeyError:
        yield 1
        see('star 1')
    except* IndexError:
        yield 2
        see('star 2')
    see('star after')
    yield 3

g = g_star()
under(ValueError('s1'), next, g)
under(None, next, g)
under(TypeError('s3'), next, g)

# 9. async generator
import asyncio

async def agen():
    try:
        raise KeyError('ag')
    except KeyError:
        yield 1
        see('agen in handler')
        await asyncio.sleep(0)
        see('agen in handler 2')
    see('agen after')
    yield 2
    see('agen after 2')

async def amain():
    it = agen()
    try:
        raise ValueError('am')
    except ValueError:
        await anext(it)
    see('amain')
    await anext(it)
    try:
        raise TypeError('am2')
    except TypeError:
        try:
            await anext(it)
        except StopAsyncIteration:
            see('amain end')

    async def worker(n):
        try:
            raise KeyError(n)
        except KeyError:
            await asyncio.sleep(0)
            see('worker %d in handler' % n)
        await asyncio.sleep(0)
        see('worker %d after' % n)
    try:
        raise ValueError('gather')
    except ValueError:
        await asyncio.gather(worker(1), worker(2))
        t = asyncio.create_task(worker(3))
    await t
    see('amain last')

asyncio.run(amain())
see('end')
