# A special method that is a descriptor written in Python is bound with its
# `__get__`, as CPython's slot lookup binds it: a `functools.partial` gets
# the instance as its first argument, and any class with `__get__` is asked
# for the callable.
import functools, asyncio, operator, copy, math
P = functools.partial
def t(tag, f):
    try:
        print(tag, f())
    except Exception as e:
        print(tag, type(e).__name__, e)
log = []
class C:
    __index__ = P(lambda self: 2)
    __getattr__ = P(lambda self, n: 'ga ' + n)
    __setattr__ = P(lambda self, n, v: log.append(('set', n, v)))
    __delattr__ = P(lambda self, n: log.append(('del', n)))
    __enter__ = P(lambda self: 'en')
    __exit__ = P(lambda self, *a: log.append('exit'))
    __aenter__ = P(lambda self: asyncio.sleep(0, 'aen'))
    __aexit__ = P(lambda self, *a: asyncio.sleep(0))
    __setitem__ = P(lambda self, k, v: log.append(('si', k, v)))
    __delitem__ = P(lambda self, k: log.append(('di', k)))
    __iadd__ = P(lambda self, o: 'iadd')
    __radd__ = P(lambda self, o: 'radd')
    __next__ = P(lambda self: 'nx')
    __reversed__ = P(lambda self: iter('zy'))
    __abs__ = P(lambda self: 'abs')
    __round__ = P(lambda self, n=None: 'round')
    __trunc__ = P(lambda self: 9)
    __floor__ = P(lambda self: 8)
    __ceil__ = P(lambda self: 7)
    __complex__ = P(lambda self: 1j)
    __ne__ = P(lambda self, o: 'ne')
    __gt__ = P(lambda self, o: 'gt')
    __missing__ = 1
    __aiter__ = P(lambda self: self)
    __anext__ = P(lambda self: asyncio.sleep(0, 'an'))
    __length_hint__ = P(lambda self: 3)
    __copy__ = P(lambda self: 'cp')
    __deepcopy__ = P(lambda self, memo: 'dcp')
    __init_subclass__ = P(lambda *a, **k: None)
    __sizeof__ = P(lambda self: 1)
    __dir__ = P(lambda self: ['d'])
    __instancecheck__ = 1
    __matmul__ = P(lambda self, o: 'mm')
    __invert__ = P(lambda self: 'inv')
    __pos__ = P(lambda self: 'pos')
    __divmod__ = P(lambda self, o: 'dm')
    __pow__ = P(lambda self, o, m=None: 'pow')
c = C()
t('index', lambda: [1, 2, 3][c])
t('op.index', lambda: operator.index(c))
t('getattr', lambda: c.zzz)
def f(): c.a = 1; del c.a; return log
t('setattr', f)
def f():
    with c as x: return x
t('with', f)
async def f():
    async with c as x: return x
t('async with', lambda: asyncio.run(f()))
def f(): c[1] = 2; del c[1]; return log
t('setitem', f)
def f():
    d = C(); d += 1; return d
t('iadd', f)
t('radd', lambda: 1 + c)
t('next', lambda: next(c))
t('reversed', lambda: list(reversed(c)))
t('abs', lambda: abs(c))
t('round', lambda: round(c))
t('round2', lambda: round(c, 2))
t('trunc', lambda: math.trunc(c))
t('floor', lambda: math.floor(c))
t('ceil', lambda: math.ceil(c))
t('complex', lambda: complex(c))
t('ne', lambda: c != 1)
t('gt', lambda: c > 1)
async def f():
    async for x in c: return x
t('async for', lambda: asyncio.run(f()))
t('hint', lambda: operator.length_hint(c))
t('copy', lambda: copy.copy(c))
t('deepcopy', lambda: copy.deepcopy(c))
t('dir', lambda: dir(c))
t('matmul', lambda: c @ 1)
t('inv', lambda: ~c)
t('pos', lambda: +c)
t('divmod', lambda: divmod(c, 1))
t('pow', lambda: c ** 2)
t('pow3', lambda: pow(c, 2, 3))
class I:
    __init__ = P(lambda self, v=1: setattr(self, 'v', v))
    __new__ = P(lambda cls, *a, **k: object.__new__(cls))
t('init', lambda: I(v=2).v)
class K(dict):
    __missing__ = P(lambda self, k: 'miss ' + k)
t('missing', lambda: K()['q'])
class S:
    __get__ = P(lambda self, obj, cls=None: 'dget')
    __set__ = P(lambda self, obj, v: log.append('dset'))
class H:
    s = S()
t('descr get', lambda: H().s)
class B:
    __bool__ = P(lambda self: True)
    __hash__ = P(lambda self: 5)
    __contains__ = P(lambda self, x: True)
    __eq__ = P(lambda self, o: True)
    __repr__ = P(lambda self: 'BR')
t('if', lambda: 'y' if B() else 'n')
t('dict key', lambda: {B(): 1}[B()])
t('in', lambda: 3 in B())
t('print', lambda: print(B(), [B()], f'{B()!r} {B()}') )
t('sorted', lambda: sorted([C(), C()], key=lambda o: 0) and 'ok')


class Desc:
    def __init__(self, r):
        self.r = r

    def __get__(self, obj, cls=None):
        print('get', type(obj).__name__, cls.__name__)
        return lambda *a, **k: (self.r, a, k)


class NoGet:
    def __call__(self, *a):
        return len(a)


class D:
    __len__ = P(lambda self, n=3: n)
    __getitem__ = Desc('gi')
    __call__ = Desc('call')
    __neg__ = Desc('neg')
    __int__ = NoGet()
    m = P(lambda self, n=3: n)


d = D()
t('len', lambda: len(d))
t('getitem', lambda: d[0])
t('call', lambda: d(1, k=2))
t('neg', lambda: -d)
# A callable without `__get__` is not bound.
t('int', lambda: int(d))
t('method', lambda: d.m())
