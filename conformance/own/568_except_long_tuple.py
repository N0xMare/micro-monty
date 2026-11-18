# `except` with a tuple around and past a native walk (16,384 items): a
# longer tuple is checked in the prelude's `_exc_match_any` a walk at a
# time, so a gate parks, snapshots and restores inside it. Every item is
# checked first, then matched, in CPython's order: a match first, last,
# in the middle and absent, by a base class and by a user class, a
# non-class item before and after the match, nested tuples, the handled
# exception and its context, generators thrown into, coroutines,
# `except*`, the recursion limit, and a tuple that is only a temporary of
# the clause.
import sys
W = 16384
def show(label, f):
    try:
        print(label, 'OK', repr(f())[:300])
    except BaseException as x:
        print(label, 'EXC', type(x).__name__, str(x)[:120], '| ctx', type(x.__context__).__name__)
class A(Exception): pass
class B(A): pass
class C(B): pass
class Other(Exception): pass
class NotExc: pass
log = []
def catch(exc, t):
    del log[:]
    try:
        try:
            raise exc
        except t as e:
            r = ('caught', type(e).__name__, e is exc if not isinstance(exc, type) else True, sys.exception() is e)
        except BaseException as e:
            r = ('later clause', type(e).__name__)
        return r + (list(log),)
    finally:
        pass
for n in (16383, 16384, 16385, 17000):
    pad = (KeyError,) * n
    L = 'n=%d ' % n
    show(L + 'first', lambda: catch(ValueError('v'), (ValueError,) + pad))
    show(L + 'last', lambda: catch(ValueError('v'), pad + (ValueError,)))
    show(L + 'middle', lambda: catch(ValueError('v'), pad[:n // 2] + (ValueError,) + pad[n // 2:]))
    show(L + 'absent', lambda: catch(ValueError('v'), pad))
    show(L + 'class raised', lambda: catch(ValueError, pad + (ValueError,)))
    show(L + 'by base', lambda: catch(IndexError(1), (OSError,) * n + (LookupError,)))
    show(L + 'by BaseException', lambda: catch(KeyboardInterrupt(), pad + (BaseException,)))
    show(L + 'user by base', lambda: catch(C('c'), (Other,) * n + (A,)))
    show(L + 'user absent', lambda: catch(C('c'), (Other,) * n))
    show(L + 'user exact among builtins', lambda: catch(C('c'), pad + (C,)))
    show(L + 'builtin among users', lambda: catch(KeyError('k'), (Other,) * n + (LookupError,)))
    show(L + 'derived listed, base raised', lambda: catch(A('a'), (C,) * n))
    bads = ((5, 'int instance'), (NotExc, 'plain class'), ((ValueError,), 'nested tuple'))
    if n == 17000:
        bads += ((int, 'int'), (None, 'None'), ('ValueError', 'str'), (ValueError('i'), 'exc instance'), ([ValueError], 'list'), (object, 'object'))
    for bad, name in bads:
        show(L + name + ' first, no match', lambda: catch(ValueError('v'), (bad,) + pad))
        show(L + name + ' last, match first', lambda: catch(ValueError('v'), (ValueError,) + pad + (bad,)))
        show(L + name + ' first, match last', lambda: catch(ValueError('v'), (bad,) + pad + (ValueError,)))
        show(L + name + ' middle, user exc', lambda: catch(C('c'), (A,) + pad[:n // 2] + (bad,) + pad[n // 2:]))
    show(L + 'nested holds the match', lambda: catch(ValueError('v'), pad + ((ValueError,),)))
    show(L + 'empty nested', lambda: catch(ValueError('v'), pad + ((),)))
    # tuples built other ways
    show(L + 'tuple(list)', lambda: catch(ValueError('v'), tuple([KeyError] * n + [ValueError])))
    show(L + 'tuple(gen)', lambda: catch(ValueError('v'), tuple(KeyError if i < n else ValueError for i in range(n + 1))))
    show(L + 'slice', lambda: catch(ValueError('v'), ((ValueError,) + pad + (ValueError,))[1:]))
    show(L + 'distinct classes', lambda: catch(C('c'), tuple(type('X%d' % i, (Exception,), {}) for i in range(n)) + (B,)))
# the handled exception and what follows the check
def state(t):
    out = []
    try:
        raise KeyError('outer')
    except KeyError as outer:
        try:
            raise ValueError('inner')
        except t as e:
            out.append((type(sys.exception()).__name__, type(e.__context__).__name__, e.__traceback__ is not None))
        out.append(type(sys.exception()).__name__)
        try:
            try:
                raise ValueError('again')
            except t:
                raise
        except ValueError as e2:
            out.append(('reraised', e2.args, type(e2.__context__).__name__))
        try:
            try:
                raise ValueError('x')
            except t:
                raise IndexError('in handler')
        except IndexError as e3:
            out.append(('raised in handler', type(e3.__context__).__name__))
        try:
            try:
                raise ValueError('x')
            except t + (5,):
                pass
        except TypeError as e4:
            out.append(('bad', type(e4.__context__).__name__, type(sys.exception()).__name__, e4.__traceback__ is not None))
    out.append(sys.exception())
    return out
show('state short', lambda: state((KeyError,) * 5 + (ValueError,)))
show('state long', lambda: state((KeyError,) * 20000 + (ValueError,)))
# finally, else, loops, with
def flow(t):
    out = []
    for i in range(3):
        try:
            if i == 1: raise ValueError(i)
            if i == 2: raise KeyError(i)
        except t as e:
            out.append(('h', i)); continue
        else:
            out.append(('else', i))
        finally:
            out.append(('fin', i))
    class CM:
        def __enter__(s): return s
        def __exit__(s, *a): out.append(('exit', a[0].__name__ if a[0] else None)); return False
    try:
        with CM():
            raise ValueError('w')
    except t:
        out.append('after with')
    return out
show('flow short', lambda: flow((IndexError,) * 5 + (LookupError, ValueError)))
show('flow long', lambda: flow((IndexError,) * 20000 + (LookupError, ValueError)))
# a generator: thrown into, and one that raises
def gens(t):
    out = []
    def g():
        while True:
            try:
                yield 'ready'
            except t as e:
                yield ('caught in gen', type(e).__name__)
            except GeneratorExit:
                raise
            except BaseException as e:
                yield ('other in gen', type(e).__name__)
    it = g(); next(it)
    out.append(it.throw(ValueError('v'))); next(it)
    out.append(it.throw(OSError('o'))); next(it)
    try:
        it.throw(ValueError('v2'), ) and None
        out.append(it.throw(KeyboardInterrupt()))
    except BaseException as x:
        out.append(('escaped', type(x).__name__))
    it.close()
    def raiser():
        yield 1
        raise ValueError('from gen')
    r = raiser(); next(r)
    try:
        next(r)
    except t as e:
        out.append(('from gen', e.args))
    def inner():
        try:
            yield 1
        except t:
            yield 'inner caught'
    def outer():
        x = yield from inner()
        yield 'outer after'
    o = outer(); next(o)
    out.append(o.throw(ValueError('d')))
    # StopIteration and GeneratorExit matched by a long tuple
    try:
        next(iter(()))
    except (KeyError,) * len(t) + (StopIteration,):
        out.append('StopIteration')
    def ge():
        try:
            yield 1
        except (KeyError,) * len(t) + (GeneratorExit,):
            out.append('GeneratorExit'); raise
    q = ge(); next(q); q.close()
    return out
show('gens short', lambda: gens((KeyError,) * 5 + (ValueError,)))
show('gens long', lambda: gens((KeyError,) * 20000 + (ValueError,)))
import asyncio
def coro(t):
    out = []
    async def fail():
        await asyncio.sleep(0)
        raise ValueError('async')
    async def one(i):
        try:
            await fail()
        except t as e:
            await asyncio.sleep(0)
            return ('caught', i, e.args)
    async def main():
        out.extend(await asyncio.gather(one(1), one(2), one(3)))
        try:
            await asyncio.gather(fail(), fail())
        except t as e:
            out.append(('gather', e.args))
        task = asyncio.ensure_future(asyncio.sleep(10))
        await asyncio.sleep(0)
        task.cancel()
        try:
            await task
        except (KeyError,) * len(t) + (asyncio.CancelledError,):
            out.append('cancelled')
    asyncio.run(main())
    return out
show('coro short', lambda: coro((KeyError,) * 5 + (ValueError,)))
show('coro long', lambda: coro((KeyError,) * 20000 + (ValueError,)))
# except* with a long tuple
def star(t, bad=None):
    out = []
    try:
        try:
            raise ExceptionGroup('g', [ValueError(1), KeyError(2), TypeError(3), ExceptionGroup('n', [ValueError(4), OSError(5)])])
        except* t as eg:
            out.append(('star', repr(eg)[:120]))
        except* OSError as eg:
            out.append(('os', repr(eg)[:80]))
    except BaseException as rest:
        out.append(('rest', type(rest).__name__, repr(rest)[:100]))
    try:
        try:
            raise ValueError('naked')
        except* t as eg:
            out.append(('naked', repr(eg)[:80]))
    except BaseException as rest:
        out.append(('rest2', type(rest).__name__))
    return out
show('star short', lambda: star((IndexError,) * 5 + (ValueError,)))
show('star long', lambda: star((IndexError,) * 20000 + (ValueError,)))
show('star long bad', lambda: star((IndexError,) * 20000 + (5,)))
show('star short bad', lambda: star((IndexError,) * 5 + (5,)))
show('star long nested', lambda: star((IndexError,) * 20000 + ((ValueError,),)))
show('star long group', lambda: star((IndexError,) * 20000 + (ExceptionGroup,)))
show('star short group', lambda: star((IndexError,) * 5 + (ExceptionGroup,)))
# at the recursion limit
def deep(t):
    def f(n):
        try:
            return f(n + 1)
        except t:
            return n
    a = f(0)
    return a > 50
show('recursion short', lambda: deep((KeyError,) * 5 + (RecursionError,)))
show('recursion long', lambda: deep((KeyError,) * 20000 + (RecursionError,)))
# many times
def many(t):
    n = 0
    for i in range(300):
        try:
            raise (ValueError, KeyError, OSError)[i % 3](i)
        except t:
            n += 1
        except OSError:
            n += 1000
    return n
show('many', lambda: many((IndexError,) * 20000 + (ValueError, LookupError)))

# the tuple is a temporary of the except clause only
def temp():
    n = 0
    for i in range(40):
        try:
            raise (ValueError, KeyError, OSError)[i % 3](i)
        except tuple([IndexError] * 17000) + (ValueError, LookupError):
            n += 1
        except OSError:
            n += 1000
    return n
show('temp tuple', temp)
def temp_gen():
    def g():
        try:
            yield 1
        except tuple([IndexError] * 17000 + [ValueError]) as e:
            yield type(e).__name__
    out = []
    for i in range(5):
        it = g(); next(it); out.append(it.throw(ValueError(i))); it.close()
    return out
show('temp tuple in gen', temp_gen)

# isinstance and issubclass against a walk's worth of items for a class
# with a deep MRO, and items whose metaclass's MRO is deep: the walk
# charges each MRO, so these go to the prelude part way.
D = Exception
for i in range(300):
    D = type('D', (D,), {})
for n in (54, 55, 16383):
    show('deep isinstance %d' % n, lambda: (isinstance(D(), (KeyError,) * n),
                                             isinstance(D(), (KeyError,) * n + (B, D)),
                                             issubclass(D, (KeyError,) * n + (Exception,))))
M = type
for i in range(200):
    M = type('M', (M,), {})
K = M('K', (), {})
for n in (81, 82, 16383):
    show('meta isinstance %d' % n, lambda: (isinstance(1, (K,) * n), isinstance(K(), (K,) * n),
                                             issubclass(int, (K,) * n + (int,))))
class MI(type):
    def __instancecheck__(cls, x):
        return x == 'yes'
KI = MI('KI', (), {})
show('meta check last', lambda: (isinstance('yes', (K,) * 100 + (KI,)), isinstance('no', (K,) * 100 + (KI,))))

# `e.args = t` keeps the tuple; a SyntaxError reads its location only from
# exactly two arguments.
def args_kept():
    t = (1, 2, 3)
    e = OSError('x')
    e.args = t
    l = [4, 5]
    e2 = ValueError()
    e2.args = l
    return e.args is t, e2.args, type(e2.args).__name__
show('args kept', args_kept)
def syntax_args():
    loc = ('f.py', 3, 4, 'txt', 3, 5)
    out = []
    for a in (('m', loc), ('m', loc, 'extra'), ('m',), ('m', loc[:4])):
        e = SyntaxError(*a)
        out.append((e.msg, e.filename, e.lineno, e.offset, e.text, e.end_lineno, len(e.args)))
    return out
show('syntax args', syntax_args)

# A class whose MRO passes a whole walk (16,386 entries): each item's
# check is one MRO scan, never more than a walk can pay, so the native
# walk and the prelude do not hand it back and forth.
import copy
def chain(k, n):
    c = object
    for i in range(n):
        c = type('K%d_%d' % (k, i), (c,), {})
    return c
Big = type('Big', tuple(chain(k, 128) for k in range(128)), {})
show('wide mro', lambda: (len(Big.__mro__), issubclass(Big, int), issubclass(Big, object),
                          issubclass(Big, (str, (int, Big))), isinstance(Big(), (str, int)),
                          isinstance(Big(), (str, Big.__mro__[-2])), issubclass(Big, (int,) * 64 + (Big,))))
show('wide mro copy', lambda: (type(copy.copy(Big())).__name__, type(copy.deepcopy(Big())).__name__))
show('wide mro bad item', lambda: issubclass(Big, (int, 1)))

# The prelude goes on from the item the native walk stopped at: checks in
# order, each once.
log = []
class ML(type):
    def __instancecheck__(cls, x):
        log.append(cls.__name__)
        return cls.__name__ == 'L2'
L1, L2, L3 = ML('L1', (), {}), ML('L2', (), {}), ML('L3', (), {})
show('resume order', lambda: (isinstance(D(), (KeyError,) * 60 + (L1, (KeyError,) * 3, L2, L3)), log))
