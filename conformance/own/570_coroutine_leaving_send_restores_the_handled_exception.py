# A coroutine first driven by send() inside the caller's handler and
# suspended inside its own handler, then run on after the caller left its
# handler: when it returns, raises, is thrown into or is closed, the
# caller's handled exception is the one it has there, not the one it had
# at the first send. A bare raise, sys.exc_info() and the __context__ of
# the next exception show it; the stale exception is not kept alive.
import gc
import sys


class Aw:
    def __await__(self):
        return (yield 'aw')


async def co(end):
    try:
        raise KeyError('own')
    except KeyError:
        try:
            await Aw()  # suspended inside its own handler
        except ZeroDivisionError:
            if end == 'throw-caught':
                return 'caught'
            raise
    if end == 'raise':
        raise LookupError('out')
    return 'ret'


def observe(tag):
    out = [tag, repr(sys.exc_info()[1])]
    try:
        raise
    except RuntimeError:
        out.append('no active exception')
    except BaseException as e:
        out.append('re-raised ' + repr(e))
    try:
        {}['missing']
    except KeyError as e:
        out.append('context ' + repr(e.__context__))
    print(*out)


def start(end):
    c = co(end)
    try:
        raise ValueError('outer')
    except ValueError:
        c.send(None)
    return c


for end in ('return', 'raise', 'throw-caught', 'throw-out', 'close'):
    c = start(end)
    try:
        if end in ('return', 'raise'):
            c.send(None)
        elif end == 'close':
            c.close()
        else:
            c.throw(ZeroDivisionError('z'))
    except StopIteration as e:
        print(end, 'stopped', e.value)
    except BaseException as e:
        # The __context__ of what the coroutine itself raises after its own
        # handler ended is not checked here: that handler restores, when it
        # ends, what it found at the first send (older; 0.0.3).
        print(end, 'raised', repr(e))
    observe(end)

# Inside a caller's later handler, the coroutine's end gives that handler's
# exception back.
c = start('return')
try:
    raise IndexError('later')
except IndexError:
    try:
        c.send(None)
    except StopIteration:
        pass
    observe('in a later handler')
observe('after it')


# The caller's exception holds 180 MiB. The coroutine finishes inside the
# caller's handler and stays alive afterwards: once the handler is done,
# neither the finished coroutine nor anything else keeps the exception
# (default limit 256 MiB).
N = 60 * 1024 * 1024


def drive():
    c = co('return')
    try:
        raise ValueError(bytes(N), bytes(N), bytes(N))
    except ValueError:
        c.send(None)
        try:
            c.send(None)
        except StopIteration as e:
            value = e.value
    return c, value


kept, value = drive()
print('large', value, sys.exc_info()[1] is None)
gc.collect()
a = bytes(N)
b = bytes(N)
d = bytes(N)
print('large ok', len(a) + len(b) + len(d), type(kept).__name__)
