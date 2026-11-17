# `copy.copy` and `copy.deepcopy` keep what CPython's reductions keep. An
# `AttributeError`'s `name` is state (`AttributeError.__reduce__`; its `obj`
# is not), set on the copy as an attribute, as `BaseException.__setstate__`
# sets every item of the state. An `OrderedDict` has no `__copy__`: its
# `__reduce__` (`odict_reduce`) calls the class with nothing, hands over the
# instance's attributes and stores each item of `items()` with `[]`, so a
# shallow copy of a subclass instance keeps its attributes. The other
# exceptions' fields and the other containers' subclasses are here as they
# copy in CPython, the ones CPython drops included.
import copy
from collections import ChainMap, Counter, OrderedDict, defaultdict, deque

BOTH = (('copy', copy.copy), ('deepcopy', copy.deepcopy))


def t(name, f):
    for how, c in BOTH:
        try:
            print(how, name, '=>', repr(f(c)))
        except BaseException as ex:
            print(how, name, 'RAISES', type(ex).__name__, ex)


def catch(f):
    try:
        f()
    except BaseException as e:
        return e


def pair(c, e):
    return e, c(e)


class Plain:
    pass


# AttributeError: `name` is kept, `obj` is not.
def named(e):
    return (type(e).__name__, e.args, e.name, e.obj, str(e), sorted(vars(e)))


class AE(AttributeError):
    pass


class AERewrites(AttributeError):
    def __init__(self, a, **k):
        super().__init__(a + '!', **k)


class AESetstate(AttributeError):
    def __setstate__(self, state):
        self.got = sorted(state)


class AEReduce(AttributeError):
    def __reduce__(self):
        return (AEReduce, ('reduced',))


def late():
    e = AttributeError('m')
    e.name = 'late'
    e.obj = [1]
    e.extra = [2]
    return e


def mutable(c):
    e, x = pair(c, AttributeError([1], name=['n'], obj=[3]))
    return (x.args, x.name, x.obj, x.args[0] is e.args[0], x.name is e.name)


def raised_again(c):
    x = c(catch(lambda: (1).zzz))
    try:
        raise x
    except AttributeError as e:
        return (e is x, e.name, e.obj, str(e))


def from_getattr(c):
    class G:
        def __getattr__(self, k):
            raise AttributeError(k, name=k, obj=self)
    x = c(catch(lambda: G().missing))
    return (x.args, x.name, x.obj)


def cycle(c):
    e = AttributeError('m', name='n')
    e.me = e
    x = c(e)
    return (x is e, x.me is x, x.me is e, x.name)


def twice(c):
    e = AttributeError('m', name='n')
    x = c([e, e])
    return (x[0] is x[1], x[0] is e, x[0].name)


def noted(c):
    e = AttributeError('m', name='n')
    e.add_note('note')
    x = c(e)
    return (x.name, x.__notes__, x.__notes__ is e.__notes__)


t('AttributeError name obj', lambda c: named(c(AttributeError('m', name='n', obj=3))))
t('AttributeError name', lambda c: named(c(AttributeError('m', name='n'))))
t('AttributeError obj', lambda c: named(c(AttributeError('m', obj=3))))
t('AttributeError plain', lambda c: named(c(AttributeError('m'))))
t('AttributeError no args', lambda c: named(c(AttributeError())))
t('AttributeError two args', lambda c: named(c(AttributeError('m', 2, name='n'))))
t('AttributeError of an int', lambda c: named(c(catch(lambda: (1).zzz))))
t('AttributeError of an instance', lambda c: named(c(catch(lambda: Plain().zzz))))
t('AttributeError of a module', lambda c: named(c(catch(lambda: copy.zzz))))
t('AttributeError from __getattr__', from_getattr)
t('AttributeError assigned', lambda c: (lambda e, x: named(x) + (x.extra, x.extra is e.extra))(*pair(c, late())))
t('AttributeError mutable', mutable)
t('AttributeError subclass', lambda c: named(c(AE('m', name='n', obj=3))))
t('AttributeError args as state', lambda c: named(c(AERewrites('m', name='n'))))
t('AttributeError __setstate__', lambda c: (lambda e: (e.args, e.name, e.got))(c(AESetstate('m', name='n'))))
t('AttributeError __reduce__', lambda c: named(c(AEReduce('m', name='n'))))
t('AttributeError raised again', raised_again)
t('AttributeError cycle', cycle)
t('AttributeError twice', twice)
t('AttributeError notes', noted)


# The state is set with `setattr`, so a class's `__setattr__` and its
# properties see it.
class Hooked(Exception):
    log = []

    def __setattr__(self, k, v):
        Hooked.log.append(k)
        super().__setattr__(k, v)


class WithProperty(Exception):
    def __init__(self, a):
        super().__init__(a)
        self._v = a

    @property
    def v(self):
        return self._v

    @v.setter
    def v(self, x):
        self._v = ('set', x)


def hooked(c):
    e = Hooked('a')
    e.x = 1
    e.y = [2]
    del Hooked.log[:]
    x = c(e)
    return (x.args, x.x, x.y, x.y is e.y, Hooked.log, sorted(vars(x)))


def with_property(c):
    e = WithProperty(1)
    e.__dict__['v'] = 'shadow'
    x = c(e)
    return (x.args, x.v, sorted(vars(x)))


t('state through __setattr__', hooked)
t('state through a property', with_property)


# The `name` an AttributeError keeps is state, not a `name` its class
# defines: a read-only property is not set again unless a name was given.
class NameProperty(AttributeError):
    @property
    def name(self):
        return 'prop'


t('AttributeError name property', lambda c: (c(NameProperty('m')).args, c(NameProperty('m')).name))
t('AttributeError name property, name given', lambda c: c(NameProperty('m', name='n')).name)


# NameError has no reduction of its own, so CPython's copy has no `name`;
# an ImportError's `name`, `path` and `name_from` are state.
def import_module():
    import zzz_no_module


def import_name():
    from math import zzz_no_name


def unbound():
    print(local)
    local = 1


def imported(e):
    return (type(e).__name__, e.args, e.name, e.path, e.name_from, e.msg, str(e))


t('NameError name', lambda c: (lambda e: (e.args, e.name))(c(NameError('m', name='n'))))
t('NameError real', lambda c: (lambda e: (e.args, e.name))(c(catch(lambda: zzz_undefined))))
t('UnboundLocalError real', lambda c: (lambda e: (e.args, e.name))(c(catch(unbound))))
t('ImportError name path', lambda c: imported(c(ImportError('i', name='nm', path='p'))))
t('ImportError name_from', lambda c: imported(c(ImportError('i', name='nm', path='p', name_from='nf'))))
t('ImportError plain', lambda c: imported(c(ImportError('i'))))
t('ImportError of a module', lambda c: imported(c(catch(import_module))))
t('ImportError of a name', lambda c: (lambda e: (e.args, e.name, e.name_from))(c(catch(import_name))))
t('ModuleNotFoundError name', lambda c: imported(c(ModuleNotFoundError('i', name='nm'))))


# Fields CPython reads from the arguments.
def os_fields(e):
    return (type(e).__name__, e.errno, e.strerror, e.filename, e.filename2, str(e))


def generator():
    return [7]
    yield


class OE(OSError):
    pass


t('KeyError', lambda c: (lambda e: (e.args, str(e), repr(e)))(c(KeyError('k'))))
t('KeyError real', lambda c: (lambda e: (e.args, str(e)))(c(catch(lambda: {}['missing']))))
t('StopIteration', lambda c: (lambda e: (e.args, e.value))(c(StopIteration(3))))
t('StopIteration none', lambda c: (lambda e: (e.args, e.value))(c(StopIteration())))
t('StopIteration real', lambda c: (lambda e: (e.args, e.value))(c(catch(lambda: next(generator())))))
t('StopAsyncIteration', lambda c: c(StopAsyncIteration(3)).args)
t('SystemExit', lambda c: (lambda e: (e.args, e.code))(c(SystemExit(4))))
t('SystemExit none', lambda c: (lambda e: (e.args, e.code))(c(SystemExit())))
t('SystemExit two', lambda c: (lambda e: (e.args, e.code))(c(SystemExit(4, 'x'))))
t('OSError none', lambda c: os_fields(c(OSError())))
t('OSError one', lambda c: os_fields(c(OSError('only'))))
t('OSError errno', lambda c: os_fields(c(OSError(5, 'io'))))
t('OSError filename', lambda c: os_fields(c(OSError(5, 'io', 'file'))))
t('OSError filename2', lambda c: os_fields(c(OSError(5, 'io', 'file', None, 'f2'))))
t('OSError subclass', lambda c: os_fields(c(OE(5, 'io', 'file'))))
t('FileNotFoundError', lambda c: os_fields(c(FileNotFoundError(2, 'nf', 'file'))))
t('SyntaxError', lambda c: (lambda s: (s.args, s.msg, s.filename, s.lineno, s.offset, s.text, str(s)))(
    c(SyntaxError('s', ('f', 1, 2, 't')))))
t('ExceptionGroup', lambda c: (lambda g: (g.args, g.message, g.exceptions))(
    c(ExceptionGroup('g', [ValueError(1), AttributeError('m', name='n')]))))
t('ExceptionGroup member name', lambda c: c(ExceptionGroup('g', [AttributeError('m', name='n')])).exceptions[0].name)


# A codec's error copies to an equal one.
def same(c, e):
    x = c(e)
    return (type(x) is type(e), x is e, x.args == e.args, str(x) == str(e), str(x))


t('UnicodeDecodeError', lambda c: same(c, catch(lambda: b'\xff'.decode('utf-8'))))
t('UnicodeEncodeError', lambda c: same(c, catch(lambda: '\xe9'.encode('ascii'))))


# OrderedDict: a subclass instance's attributes are its state.
class OD(OrderedDict):
    pass


class ODLog(OrderedDict):
    def __init__(self, *a, **k):
        self.log = []
        super().__init__(*a, **k)

    def __setitem__(self, k, v):
        self.log.append(k)
        super().__setitem__(k, v)


class ODNeedsArg(OrderedDict):
    def __init__(self, need, **k):
        self.need = need
        super().__init__(**k)


class ODItems(OrderedDict):
    def items(self):
        return [('via', 'items')]


class ODGetstate(OrderedDict):
    def __getstate__(self):
        return {'from': 'getstate'}


class ODSetstate(OrderedDict):
    def __setstate__(self, state):
        self.__dict__['got'] = sorted(state)


def tagged(o):
    o.tag = ['t']
    o.n = 5
    return o


def attrs(c, o):
    x = c(o)
    return (type(x).__name__, list(x.items()), sorted(vars(x)), x.tag, x.n, x.tag is o.tag, x is o)


def logged(c):
    o = tagged(ODLog(a=[1], b=2))
    x = c(o)
    return (list(x.items()), x.tag is o.tag, x.log, o.log, x.log is o.log)


def od_cycle(c):
    o = OD()
    o['self'] = o
    o.me = o
    x = c(o)
    return (x is o, x['self'] is o, x['self'] is x, x.me is o, x.me is x)


def moved(c):
    o = tagged(OD(a=1, b=2, c=3))
    o.move_to_end('a')
    return list(c(o))


def exact(c):
    o = OrderedDict(b=2, a=[1])
    x = c(o)
    return (type(x).__name__, list(x.items()), x == o, x is o, x['a'] is o['a'])


def reduction(o):
    cls, args, state, items, pairs = o.__reduce__()
    return (cls.__name__, args, state, items, list(pairs))


t('OrderedDict subclass', lambda c: attrs(c, tagged(OD(a=[1], b=2))))
t('OrderedDict subclass, empty', lambda c: attrs(c, tagged(OD())))
t('OrderedDict subclass, no attributes', lambda c: (lambda x: (list(x.items()), vars(x)))(c(OD(a=1))))
t('OrderedDict __setitem__', logged)
t('OrderedDict __init__ needs an argument', lambda c: (lambda x: (list(x.items()), x.need))(c(ODNeedsArg(7, a=1))))
t('OrderedDict items()', lambda c: list(dict.items(c(ODItems(a=1)))))
t('OrderedDict __getstate__', lambda c: (lambda x: (list(x.items()), vars(x)))(c(ODGetstate(a=1))))
t('OrderedDict __setstate__', lambda c: (lambda x: (list(x.items()), vars(x)))(c(tagged(ODSetstate(a=1)))))
t('OrderedDict cycle', od_cycle)
t('OrderedDict order', moved)
t('OrderedDict exact', exact)
t('OrderedDict empty', lambda c: c(OrderedDict()))
print(hasattr(OrderedDict, '__copy__'), reduction(tagged(OD(a=1))), reduction(OrderedDict(a=1)))
print(vars(tagged(OD(a=1)).copy()), ODLog(a=1, b=2).copy().log)


# The other containers' subclasses: a shallow copy keeps the attributes of
# a dict, list, set or tuple subclass, and not those of a defaultdict,
# Counter, deque or ChainMap subclass, whose `__copy__` builds a new one; a
# deep copy keeps a deque's and a ChainMap's too.
class DD(defaultdict):
    pass


class CC(Counter):
    pass


class DQ(deque):
    pass


class CM(ChainMap):
    pass


class D(dict):
    pass


class L(list):
    pass


class S(set):
    pass


class T(tuple):
    pass


def kept(c, o):
    o.tag = ['t']
    x = c(o)
    body = list(x.items()) if hasattr(x, 'items') else sorted(x)
    return (type(x).__name__, body, getattr(x, 'tag', 'dropped'), getattr(x, 'tag', None) is o.tag)


for how, c in BOTH:
    for mk in (lambda: DD(list, a=1), lambda: DQ([2, 1]), lambda: DQ([2, 1], 5), lambda: CM({'a': 1}),
               lambda: D(a=1), lambda: L([2, 1]), lambda: S([1, 2]), lambda: T([2, 1])):
        print(how, kept(c, mk()))
print('copy', kept(copy.copy, CC('aab')))
print(copy.copy(DD(list, a=1)).default_factory, copy.deepcopy(DQ([1], 5)).maxlen)
