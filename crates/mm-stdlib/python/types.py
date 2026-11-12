# types: the runtime type objects of CPython 3.14's Lib/types.py (PSF
# license, see THIRD_PARTY.md) that micro-monty has, plus MappingProxyType
# and SimpleNamespace written here (CPython's are C).

import sys
import _mm


def _f():
    pass


async def _c():
    pass


def _g():
    yield 1


class _C:
    def _m(self):
        pass


FunctionType = type(_f)
LambdaType = type(lambda: None)
GeneratorType = type(_g())
_co = _c()
CoroutineType = type(_co)
MethodType = type(_C()._m)
BuiltinFunctionType = type(len)
BuiltinMethodType = type([].append)
ModuleType = type(sys)
NoneType = type(None)
EllipsisType = type(...)
NotImplementedType = type(NotImplemented)
GenericAlias = type(list[int])
UnionType = type(int | str)
del _f, _c, _g, _C, _co


class MappingProxyType:
    """A read-only view of a mapping."""
    __mm_tpname__ = 'mappingproxy'

    def __new__(cls, mapping):
        if not hasattr(mapping, 'keys') or not hasattr(mapping, '__getitem__') or isinstance(mapping, (list, tuple)):
            raise TypeError(f'mappingproxy() argument must be a mapping, not {type(mapping).__name__}')
        self = object.__new__(cls)
        object.__setattr__(self, '_m', mapping)
        return self

    def __setattr__(self, name, value):
        raise AttributeError(f"'mappingproxy' object has no attribute '{name}'")

    def __getitem__(self, key):
        return self._m[key]

    def __iter__(self):
        return iter(self._m)

    def __len__(self):
        return len(self._m)

    def __contains__(self, key):
        return key in self._m

    def __eq__(self, other):
        return self._m == other

    def __or__(self, other):
        return dict(self._m) | other

    def __ror__(self, other):
        return other | dict(self._m)

    def __repr__(self):
        return f'mappingproxy({self._m!r})'

    def get(self, key, default=None):
        return self._m.get(key, default)

    def keys(self):
        return self._m.keys()

    def values(self):
        return self._m.values()

    def items(self):
        return self._m.items()

    def copy(self):
        return self._m.copy()


class SimpleNamespace:
    def __init__(self, mapping_or_iterable=(), /, **kwargs):
        self.__dict__.update(dict(mapping_or_iterable, **kwargs))

    def __repr__(self):
        items = ', '.join(f'{k}={v!r}' for k, v in self.__dict__.items())
        return f'namespace({items})'

    def __eq__(self, other):
        if isinstance(self, SimpleNamespace) and isinstance(other, SimpleNamespace):
            return self.__dict__ == other.__dict__
        return NotImplemented

    def __replace__(self, **changes):
        ns = type(self)()
        ns.__dict__.update(self.__dict__)
        ns.__dict__.update(changes)
        return ns


def new_class(name, bases=(), kwds=None, exec_body=None):
    ns = {}
    if exec_body is not None:
        exec_body(ns)
    return type(name, tuple(bases), ns, **(kwds or {}))


def resolve_bases(bases):
    out = []
    for b in bases:
        if not isinstance(b, type) and hasattr(b, '__mro_entries__'):
            out.extend(b.__mro_entries__(bases))
        else:
            out.append(b)
    return tuple(out)


def get_original_bases(cls, /):
    try:
        return cls.__dict__.get('__orig_bases__', cls.__bases__)
    except AttributeError:
        raise TypeError(f'Expected an instance of type, not {type(cls).__name__!r}') from None


class _GeneratorWrapper:
    # What types.coroutine returns for a callable whose result is a
    # generator it did not mark.
    def __init__(self, gen):
        self.__wrapped = gen
        self.__isgen = gen.__class__ is GeneratorType
        self.__name__ = getattr(gen, '__name__', None)
        self.__qualname__ = getattr(gen, '__qualname__', None)

    def send(self, val):
        return self.__wrapped.send(val)

    def throw(self, tp, *rest):
        return self.__wrapped.throw(tp, *rest)

    def close(self):
        return self.__wrapped.close()

    def __next__(self):
        return next(self.__wrapped)

    def __iter__(self):
        if self.__isgen:
            return self.__wrapped
        return self

    __await__ = __iter__


def coroutine(func):
    """Convert regular generator function to a coroutine."""
    if not callable(func):
        raise TypeError('types.coroutine() expects a callable')
    kind = _mm.code_kind(func)
    if kind == 3:
        return func
    if kind == 2:
        # CPython sets CO_ITERABLE_COROUTINE on the code; await then
        # accepts its generators.
        func.__mm_iterable_coroutine__ = True
        return func
    import functools

    @functools.wraps(func)
    def wrapped(*args, **kwargs):
        coro = func(*args, **kwargs)
        if coro.__class__ is GeneratorType:
            return _GeneratorWrapper(coro)
        return coro

    return wrapped
