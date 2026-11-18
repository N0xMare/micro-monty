# Class creation whose C3 merge is too long for one native step goes to
# the prelude's `_class_big` (`Vm::merge_is_long`: about a million list
# entries scanned): many bases, bases with long MROs, and several chains.
# The results, the MRO, its errors (duplicate bases, an inconsistent MRO,
# a base that cannot be one) and the class's own setup (`__init_subclass__`,
# `__set_name__`, a metaclass, `__class__`, keywords) against python3.14, on
# both sides of the threshold.
import sys
def show(label, f):
    try:
        r = f()
        print(label, 'OK', r)
    except BaseException as x:
        print(label, 'EXC', type(x).__name__, str(x)[:200])
def names(c, n=6):
    m = c.__mro__
    return (len(m), [k.__name__ for k in m[:n]], [k.__name__ for k in m[-n:]])
for K in (300, 1100):
    bs = tuple(type('B%d' % i, (Exception,), {}) for i in range(K))
    show('wide %d' % K, lambda: names(type('W', bs, {})))
    show('wide rev %d' % K, lambda: names(type('W', bs[::-1], {})))
    show('wide dup %d' % K, lambda: type('W', bs + (bs[K // 2],), {}))
    show('wide dup first %d' % K, lambda: type('W', (bs[0],) + bs, {}))
    show('wide bool %d' % K, lambda: type('W', bs + (bool,), {}))
    show('wide dup then bool %d' % K, lambda: type('W', (bs[0],) + bs + (bool,), {}))
    D = type('D', (bs[0],), {})
    show('wide inconsistent %d' % K, lambda: type('W', bs + (D,), {}))
    show('wide consistent %d' % K, lambda: names(type('W', (D,) + bs, {})))
    show('wide builtins %d' % K, lambda: names(type('W', bs + (ValueError, KeyError, object), {})))
    show('wide object first %d' % K, lambda: type('W', (object,) + bs, {}))
    class Base: pass
    ps = tuple(type('P%d' % i, (Base,), {'v': i}) for i in range(K))
    W = type('W', ps, {'w': 1})
    show('plain %d' % K, lambda: (names(W), W().v, W.w, isinstance(W(), Base), issubclass(W, ps[-1])))
# fewer bases than the native cut whose merge is long all the same
bs = tuple(type('B%d' % i, (Exception,), {}) for i in range(800))
show('wide 800', lambda: names(type('W', bs, {})))
show('wide dup then bool 800', lambda: type('W', (bs[0],) + bs + (bool,), {}))
# a long chain under several bases
C = Exception
chain = []
for i in range(100):
    C = type('C', (C,), {})
    chain.append(C)
subs = tuple(type('S%d' % i, (C,), {}) for i in range(300))
show('chain subs', lambda: names(type('W', subs, {})))
show('chain subs and mid', lambda: type('W', subs + (chain[10],), {}).__mro__[-4].__name__)
show('chain mid first', lambda: type('W', (chain[10],) + subs, {}))
heads = []
for k in range(300):
    C = Exception
    for i in range(15):
        C = type('H%d_%d' % (k, i), (C,), {})
    heads.append(C)
show('chains', lambda: names(type('W', tuple(heads) * 1, {})))
# class statement, __init_subclass__, __set_name__, metaclass, super
log = []
class Meta(type):
    def __new__(m, name, bases, ns, **kw):
        log.append(('meta', name, len(bases), kw))
        return super().__new__(m, name, bases, ns)
class Root:
    def __init_subclass__(cls, **kw):
        log.append(('isc', cls.__name__, kw))
        super().__init_subclass__()
class Desc:
    def __set_name__(self, owner, name):
        log.append(('set_name', owner.__name__, name))
rs = tuple(type('R%d' % i, (Root,), {}) for i in range(1100))
del log[:]
class Big(*rs, tag=1):
    d = Desc()
    def who(self):
        return __class__.__name__, super().who if hasattr(super(), 'who') else None
print(len(Big.__mro__), Big().who(), log)
ms = tuple(Meta('M%d' % i, (), {}) for i in range(1100))
del log[:]
class BigM(*ms, flag=2): pass
print(type(BigM).__name__, len(BigM.__mro__), log)
try:
    class Bad(*rs, rs[3]): pass
except TypeError as e:
    print('stmt dup', e)
# a metaclass without `__new__`: `type.__new__` hands off inside its call
class MI(type):
    def __init__(cls, name, bases, ns, **kw):
        log.append(('init', name, len(bases)))
        super().__init__(name, bases, ns)
mis = tuple(MI('I%d' % i, (Exception,), {}) for i in range(1100))
del log[:]
W = type('W', mis, {})
print('type()', type(W).__name__, len(W.__mro__), log)
del log[:]
W = MI('W', mis, {'x': 1})
print('MI()', type(W).__name__, len(W.__mro__), W.x, log)
del log[:]
class S(*mis):
    def f(self):
        return __class__.__name__
print('stmt', type(S).__name__, len(S.__mro__), S().f(), log)
show('MI dup', lambda: MI('W', mis + (mis[1],), {}))
# a guest's heapq does not run inside class creation
import heapq
def patched():
    old = heapq.heappop, heapq.heappush
    def pop(h):
        log.append('guest heappop')
        return old[0](h)
    heapq.heappop = pop
    heapq.heappush = pop
    try:
        del log[:]
        return len(type('W', ms[:1100], {}).__mro__), log
    finally:
        heapq.heappop, heapq.heappush = old
show('heapq patched', patched)
# a builtin __init__ whose call hands off: `next` over a Python __next__
def gen():
    yield 1
class CN:
    __init__ = next
def caught():
    try:
        return CN(gen(), None)
    except BaseException as e:
        return 'caught ' + type(e).__name__
print('init next generator', caught())
class ItU:
    def __next__(self):
        return None
show('init next user', lambda: type(CN(ItU(), None)).__name__)
class It3:
    def __next__(self, extra):
        return 1
show('init next arity', lambda: CN(It3(), None))
print('end')
