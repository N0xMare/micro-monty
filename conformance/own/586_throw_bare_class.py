# `throw(cls)` with a bare class: CPython calls the class where the
# exception is raised, inside the generator. A constructor that fails
# (`UnicodeDecodeError()` takes five arguments, a user `__init__` that needs
# one) throws its error into the generator, and a user `__init__`,
# `__new__` or metaclass `__call__` runs before the generator sees the
# instance. The same for a coroutine's `throw` and an async generator's
# `athrow`.
def t(tag, f):
    try:
        print(tag, f())
    except BaseException as e:
        print(tag, 'raised', type(e).__name__, e, '| ctx', repr(e.__context__))

def gen(log):
    while True:
        try:
            yield 1
        except BaseException as e:
            log.append(('in gen', type(e).__name__, str(e), e.args))
            if isinstance(e, GeneratorExit):
                raise

class NeedsArg(Exception):
    def __init__(self, a):
        print('init ran')
        super().__init__(a)

class Inits(Exception):
    def __init__(self):
        print('Inits init')
        super().__init__('from init')

class News(Exception):
    def __new__(cls, *a):
        print('new ran', a)
        return super().__new__(cls, *a)

class BadInit(Exception):
    def __init__(self):
        raise KeyError('init failed')

class Meta(type):
    def __call__(cls, *a):
        print('meta call')
        return type.__call__(cls, 'from meta')
class ViaMeta(Exception, metaclass=Meta): pass

for cls in [ValueError, UnicodeDecodeError, UnicodeEncodeError, NeedsArg, Inits, News, BadInit, KeyError, StopIteration, OSError, ExceptionGroup, ViaMeta, SystemExit, ImportError]:
    log = []
    g = gen(log)
    next(g)
    t(cls.__name__, lambda: g.throw(cls))
    print('  ', log, )
    t('  next', lambda: next(g))

# not started
for cls in [ValueError, UnicodeDecodeError, NeedsArg, Inits]:
    g = gen([])
    t('unstarted ' + cls.__name__, lambda: g.throw(cls))
    t('  next', lambda: next(g, 'done'))

# finished
def empty():
    return
    yield
g = empty()
next(g, None)
t('finished', lambda: g.throw(UnicodeDecodeError))
t('finished NeedsArg', lambda: g.throw(NeedsArg))

# not exceptions
g = gen([]); next(g)
t('int', lambda: g.throw(1))
t('str class', lambda: g.throw(str))
t('instance', lambda: g.throw(ValueError('inst')))

# yield from delegation
def outer(log):
    try:
        yield from gen(log)
    except BaseException as e:
        log.append(('outer', type(e).__name__))
        yield 'outer caught'
log = []
g = outer(log); next(g)
t('delegated', lambda: g.throw(UnicodeDecodeError))
print(log)
t('delegated NeedsArg', lambda: g.throw(NeedsArg))
print(log)
t('close', g.close)

# coroutine and async generator
class Aw:
    def __await__(self):
        try:
            yield 'aw'
        except BaseException as e:
            print('in await', type(e).__name__, e)
        return 5
async def co():
    return await Aw()
c = co(); c.send(None)
t('coro', lambda: c.throw(UnicodeDecodeError))
c = co(); c.send(None)
t('coro NeedsArg', lambda: c.throw(NeedsArg))
async def agen():
    try:
        yield 1
    except BaseException as e:
        print('in agen', type(e).__name__, e)
    yield 2
import asyncio
async def main():
    a = agen(); await anext(a)
    t('agen', lambda: None)
    try:
        print(await a.athrow(UnicodeDecodeError))
    except BaseException as e:
        print('athrow raised', type(e).__name__, e)
    a = agen(); await anext(a)
    try:
        print(await a.athrow(Inits))
    except BaseException as e:
        print('athrow raised', type(e).__name__, e)
asyncio.run(main())
