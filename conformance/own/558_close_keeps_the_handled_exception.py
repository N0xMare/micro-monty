# close() on a generator that has not started, or has finished, runs no
# frame, so the exception being handled where it is called stays handled:
# sys.exc_info() still names it and a bare raise re-raises it. The same
# for throw() into a generator that has not started, for close() of a
# suspended one (which runs its frame), and inside a nested handler.

import sys


def gen():
    yield 1


async def coro():
    return 1


def t(label, act):
    try:
        try:
            raise ValueError('orig')
        except ValueError:
            act()
            print(label, 'exc_info', type(sys.exc_info()[1]).__name__)
            raise
    except Exception as e:
        print(label, type(e).__name__, e, type(e.__context__).__name__)


def created_close():
    g = gen()
    g.close()


def done_close():
    g = gen()
    list(g)
    g.close()


def closed_twice():
    g = gen()
    g.close()
    g.close()


def created_throw():
    g = gen()
    try:
        g.throw(KeyError('k'))
    except KeyError:
        pass


def suspended_close():
    g = gen()
    next(g)
    g.close()


def coroutine_close():
    c = coro()
    c.close()


def nested():
    try:
        raise KeyError('inner')
    except KeyError:
        g = gen()
        g.close()
        print('nested inner exc_info', type(sys.exc_info()[1]).__name__)


def in_generator():
    def outer():
        try:
            raise TypeError('in gen')
        except TypeError:
            g = gen()
            g.close()
            yield type(sys.exc_info()[1]).__name__

    print('in generator', list(outer()))


t('created close', created_close)
t('done close', done_close)
t('closed twice', closed_twice)
t('created throw', created_throw)
t('suspended close', suspended_close)
t('coroutine close', coroutine_close)
t('nested', nested)
t('in generator', in_generator)
