# A coroutine first driven by send() inside a handler does not take the
# handled exception as its own: after the handler sys.exc_info() is
# empty, an exception thrown into the coroutine has no __context__, and a
# bare raise has no active exception, at each step and after close(). As
# for a generator, whose frame keeps only what it handles itself.
import sys


class Aw:
    def __await__(self):
        return (yield 'aw')


async def co():
    await Aw()
    await Aw()


def info():
    return repr(sys.exc_info()[1])


c = co()
try:
    raise ValueError('orig')
except ValueError:
    c.send(None)
    print('in handler', info())
print('after handler', info())
c.send(None)
print('after send outside', info())
c.close()
print('after close', info())

c2 = co()
try:
    raise KeyError('k')
except KeyError:
    c2.send(None)
try:
    c2.throw(ZeroDivisionError('z'))
except ZeroDivisionError as e:
    print('thrown out', repr(e.__context__))
print('after throw', info())
try:
    raise
except RuntimeError as e:
    print('bare raise:', e)
except BaseException as e:
    print('bare raise re-raised', repr(e))

c4 = co()
try:
    raise ValueError('a')
except ValueError:
    try:
        raise KeyError('b')
    except KeyError:
        c4.send(None)
    print('nested, back in outer', info())
c4.close()
print('after nested', info())
