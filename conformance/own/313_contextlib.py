# contextlib: contextmanager, suppress, ExitStack, AsyncExitStack, closing,
# nullcontext and redirect_stdout/stderr (docs/spec.md §4, §12).
import asyncio
import io
import sys
from contextlib import (AbstractContextManager, AsyncExitStack, ContextDecorator, ExitStack,
                        closing, contextmanager, nullcontext, redirect_stderr, redirect_stdout,
                        suppress)

log = []


@contextmanager
def tag(name):
    log.append('<' + name)
    try:
        yield name.upper()
    finally:
        log.append(name + '>')


with tag('a') as v, tag('b') as w:
    log.append(v + w)
print(log)


@contextmanager
def swallow():
    try:
        yield
    except ValueError as e:
        print('swallowed', e)


with swallow():
    raise ValueError('boom')
print('after swallow')

try:
    with swallow():
        raise KeyError('k')
except KeyError as e:
    print('passed through', repr(e))


@contextmanager
def no_yield():
    if False:
        yield


try:
    with no_yield():
        pass
except RuntimeError as e:
    print('RuntimeError', e, e.__cause__, e.__suppress_context__)


@contextmanager
def two_yields():
    yield 1
    yield 2


try:
    with two_yields():
        pass
except RuntimeError as e:
    print('RuntimeError', e)


@contextmanager
def keeps_going():
    try:
        yield
    except ZeroDivisionError:
        pass
    yield


try:
    with keeps_going():
        1 / 0
except RuntimeError as e:
    print('RuntimeError', e)


@contextmanager
def replace():
    try:
        yield
    except ValueError as e:
        raise TypeError('replaced') from e


try:
    with replace():
        raise ValueError('orig')
except TypeError as e:
    print(repr(e), repr(e.__cause__))


@contextmanager
def stop_iter():
    yield


try:
    with stop_iter():
        raise StopIteration('si')
except StopIteration as e:
    print('StopIteration propagates', e)


# A contextmanager is also a decorator, re-created for each call.
@tag('deco')
def decorated(x):
    log.append(('call', x))
    return x * 2


log.clear()
print(decorated(3), decorated(4), log)
print(decorated.__name__, tag.__name__)


class Mine(ContextDecorator):
    def __enter__(self):
        print('enter Mine')
        return self

    def __exit__(self, *exc):
        print('exit Mine', exc[0])
        return False


@Mine()
def f():
    print('in f')


f()

# suppress
with suppress(KeyError, IndexError):
    [][1]
print('suppressed')
with suppress():
    pass
try:
    with suppress(KeyError):
        raise ValueError('not suppressed')
except ValueError as e:
    print('ValueError', e)
with suppress(Exception):
    raise ZeroDivisionError

# closing and nullcontext


class Thing:
    def close(self):
        print('closed')


with closing(Thing()) as t:
    print(type(t).__name__)
with nullcontext(42) as n:
    print('null', n)
with nullcontext() as n:
    print('null', n)

# ExitStack
with ExitStack() as stack:
    for i in range(3):
        stack.callback(print, 'callback', i)
    stack.enter_context(tag('x'))
    print('body', log[-1])
print(log[-1])

with ExitStack() as stack:
    stack.push(lambda *exc: print('push saw', exc[0]) or True)
    raise KeyError('eaten')
print('ExitStack suppressed')

stack = ExitStack()
stack.callback(print, 'deferred')
moved = stack.pop_all()
stack.close()
print('nothing yet')
moved.close()

try:
    with ExitStack() as stack:
        def raiser():
            raise ValueError('from callback')
        stack.callback(raiser)
        raise KeyError('body')
except ValueError as e:
    print(repr(e), repr(e.__context__))

try:
    with ExitStack() as stack:
        stack.enter_context(42)
except TypeError as e:
    print('TypeError', e)


class Exit:
    def __exit__(self, *exc):
        print('Exit', exc[0])


with ExitStack() as stack:
    stack.push(Exit())
print(isinstance(ExitStack(), AbstractContextManager), isinstance(Exit(), AbstractContextManager))


class Both:
    def __enter__(self):
        return self

    def __exit__(self, *a):
        pass


print(isinstance(Both(), AbstractContextManager), issubclass(Both, AbstractContextManager))

# redirect_stdout / redirect_stderr
buf = io.StringIO()
with redirect_stdout(buf) as target:
    print('captured', 1, 2, sep='-')
    print('more', end='!')
print(target is buf, repr(buf.getvalue()))
err = io.StringIO()
with redirect_stderr(err):
    print('to stderr', file=sys.stderr)
print(repr(err.getvalue()))
outer, inner = io.StringIO(), io.StringIO()
with redirect_stdout(outer):
    print('outer')
    with redirect_stdout(inner):
        print('inner')
    print('outer again')
print(repr(outer.getvalue()), repr(inner.getvalue()))


# AsyncExitStack, awaited explicitly
class ACM:
    def __init__(self, name):
        self.name = name

    async def __aenter__(self):
        print('aenter', self.name)
        return self.name

    async def __aexit__(self, *exc):
        print('aexit', self.name, exc[0])
        return False


async def cleanup(what):
    print('async cleanup', what)


async def main():
    stack = AsyncExitStack()
    await stack.__aenter__()
    print(await stack.enter_async_context(ACM('one')))
    stack.push_async_callback(cleanup, 'cb')
    stack.callback(print, 'sync cb')
    print(stack.enter_context(nullcontext('sync cm')))
    await stack.aclose()
    stack = AsyncExitStack()
    await stack.enter_async_context(ACM('two'))
    try:
        raise KeyError('k')
    except KeyError as e:
        print('aexit returns', await stack.__aexit__(type(e), e, None))
    try:
        await stack.enter_async_context(Thing())
    except TypeError as e:
        print('TypeError', e)
    print(await nullcontext(5).__aenter__())


asyncio.run(main())
