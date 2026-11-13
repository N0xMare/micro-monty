# copy, adapted from CPython 3.14's Lib/copy.py (PSF license, see
# THIRD_PARTY.md). micro-monty has no pickle protocol on `object`, so
# `_reduce` stands in for `object.__reduce_ex__(4)`: a user `__reduce_ex__`
# or `__reduce__` when a class defines one, else `__getnewargs__`,
# `__getstate__`/`__dict__`, and the list/dict items of builtin subclasses.

import sys
from _mm import backing as _backing


class Error(Exception):
    pass


error = Error

__all__ = ["Error", "copy", "deepcopy", "replace"]


def _func():
    pass


_NoneType = type(None)
_FunctionType = type(_func)
_BuiltinType = type(len)
del _func


class _M:
    def m(self):
        pass


_MethodType = type(_M().m)
del _M

_copy_atomic_types = {_NoneType, int, float, bool, complex, str, tuple, bytes,
                      frozenset, type,
                      range, slice, property, _BuiltinType,
                      type(...), type(NotImplemented), _FunctionType}
_copy_builtin_containers = {list, dict, set, bytearray}


def copy(x):
    """Shallow copy operation on arbitrary Python objects."""
    cls = type(x)

    if cls in _copy_atomic_types:
        return x
    if cls in _copy_builtin_containers:
        return cls.copy(x)
    if issubclass(cls, type):
        return x
    if cls is _MethodType:
        return x

    copier = getattr(cls, "__copy__", None)
    if copier is not None:
        return copier(x)

    rv = _reduce(x)
    if isinstance(rv, str):
        return x
    return _reconstruct(x, None, *rv)


_atomic_types = {_NoneType, type(...), type(NotImplemented), int, float, bool,
                 complex,
                 bytes, str, type, range, _BuiltinType, _FunctionType,
                 property}

_depth = [0]


def deepcopy(x, memo=None, _nil=[]):
    """Deep copy operation on arbitrary Python objects."""
    cls = type(x)

    if cls in _atomic_types:
        return x

    d = id(x)
    if memo is None:
        memo = {}
    else:
        y = memo.get(d, _nil)
        if y is not _nil:
            return y

    # CPython recurses in Python here, two frames per level; the bundled
    # module's frames are not counted, so the depth is checked explicitly.
    if (_depth[0] + 1) * 2 > sys.getrecursionlimit():
        raise RecursionError('maximum recursion depth exceeded')
    _depth[0] += 1
    try:
        copier = _deepcopy_dispatch.get(cls)
        if copier is not None:
            y = copier(x, memo)
        else:
            if issubclass(cls, type):
                y = x  # atomic copy
            else:
                copier = getattr(x, "__deepcopy__", None)
                if copier is not None:
                    y = copier(memo)
                else:
                    rv = _reduce(x)
                    if isinstance(rv, str):
                        y = x
                    else:
                        y = _reconstruct(x, memo, *rv)
    finally:
        _depth[0] -= 1

    # If is its own copy, don't memoize.
    if y is not x:
        memo[d] = y
        _keep_alive(x, memo)  # Make sure x lives at least as long as d
    return y


_deepcopy_dispatch = d = {}


def _deepcopy_list(x, memo, deepcopy=deepcopy):
    y = []
    memo[id(x)] = y
    append = y.append
    for a in x:
        append(deepcopy(a, memo))
    return y


d[list] = _deepcopy_list


def _deepcopy_tuple(x, memo, deepcopy=deepcopy):
    y = [deepcopy(a, memo) for a in x]
    # We're not going to put the tuple in the memo, but it's still important we
    # check for it, in case the tuple contains recursive mutable structures.
    try:
        return memo[id(x)]
    except KeyError:
        pass
    for k, j in zip(x, y):
        if k is not j:
            y = tuple(y)
            break
    else:
        y = x
    return y


d[tuple] = _deepcopy_tuple


def _deepcopy_dict(x, memo, deepcopy=deepcopy):
    y = {}
    memo[id(x)] = y
    for key, value in x.items():
        y[deepcopy(key, memo)] = deepcopy(value, memo)
    return y


d[dict] = _deepcopy_dict


def _deepcopy_set(x, memo, deepcopy=deepcopy):
    items = list(x)
    y = set()
    memo[id(x)] = y
    for a in items:
        y.add(deepcopy(a, memo))
    return y


d[set] = _deepcopy_set


def _deepcopy_frozenset(x, memo):
    # CPython rebuilds a frozenset from its reduction, `(frozenset, (items,))`.
    y = frozenset(deepcopy(list(x), memo))
    memo[id(x)] = y
    return y


d[frozenset] = _deepcopy_frozenset


def _deepcopy_bytearray(x, memo):
    # Bytes hold no references: a copy, a step at a time when long.
    y = x.copy()
    memo[id(x)] = y
    return y


d[bytearray] = _deepcopy_bytearray


def _deepcopy_method(x, memo):  # Copy instance methods
    return type(x)(x.__func__, deepcopy(x.__self__, memo))


d[_MethodType] = _deepcopy_method

del d


def _keep_alive(x, memo):
    try:
        memo[id(memo)].append(x)
    except KeyError:
        memo[id(memo)] = [x]


def _newobj(cls, *args):
    return cls.__new__(cls, *args)


def _reduce(x):
    cls = type(x)
    reductor = getattr(cls, "__reduce_ex__", None)
    if reductor is not None:
        return reductor(x, 4)
    reductor = getattr(cls, "__reduce__", None)
    if reductor is not None:
        return reductor(x)
    # A builtin type (no class `__dict__` in micro-monty) has no default
    # reduction here; a user class may well live in `builtins` (code run
    # without a `__name__`).
    if getattr(cls, '__dict__', None) is None or cls is type(sys):
        raise TypeError(f"cannot pickle '{cls.__name__}' object")
    getnewargs = getattr(cls, "__getnewargs__", None)
    if getnewargs is not None:
        args = getnewargs(x)
    else:
        args = ()
        backing = _backing(x)
        if backing is not x and isinstance(backing, (tuple, str, bytes, int, float)):
            args = (backing,)
        elif isinstance(x, set):
            args = (list(x),)
    getstate = getattr(cls, "__getstate__", None)
    if getstate is not None:
        state = getstate(x)
    else:
        try:
            state = x.__dict__
        except AttributeError:
            state = None
        if not state:
            state = None
    listiter = iter(x) if isinstance(x, list) else None
    dictiter = iter(x.items()) if isinstance(x, dict) else None
    return (_newobj, (cls,) + tuple(args), state, listiter, dictiter)


def _reconstruct(x, memo, func, args,
                 state=None, listiter=None, dictiter=None,
                 *, deepcopy=deepcopy):
    deep = memo is not None
    if deep and args:
        args = [deepcopy(arg, memo) for arg in args]
    y = func(*args)
    if deep:
        memo[id(x)] = y

    if state is not None:
        if deep:
            state = deepcopy(state, memo)
        if hasattr(y, '__setstate__'):
            y.__setstate__(state)
        else:
            if isinstance(state, tuple) and len(state) == 2:
                state, slotstate = state
            else:
                slotstate = None
            if state is not None:
                y.__dict__.update(state)
            if slotstate is not None:
                for key, value in slotstate.items():
                    setattr(y, key, value)

    if listiter is not None:
        if deep:
            for item in listiter:
                item = deepcopy(item, memo)
                y.append(item)
        else:
            for item in listiter:
                y.append(item)
    if dictiter is not None:
        if deep:
            for key, value in dictiter:
                key = deepcopy(key, memo)
                value = deepcopy(value, memo)
                y[key] = value
        else:
            for key, value in dictiter:
                y[key] = value
    return y


def replace(obj, /, **changes):
    """Return a new object replacing specified fields with new values."""
    cls = type(obj)
    func = getattr(cls, '__replace__', None)
    if func is None:
        raise TypeError(f"replace() does not support {cls.__name__} objects")
    return func(obj, **changes)
