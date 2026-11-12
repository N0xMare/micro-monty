# functools, adapted from CPython 3.14's Lib/functools.py (PSF license, see
# THIRD_PARTY.md). partial and the lru_cache wrapper are classes standing in
# for CPython's C types (same names, reprs and messages); reduce, cmp_to_key
# and the rest follow the pure-Python versions.

from collections import namedtuple

__all__ = ['update_wrapper', 'wraps', 'WRAPPER_ASSIGNMENTS', 'WRAPPER_UPDATES',
           'total_ordering', 'cache', 'cmp_to_key', 'lru_cache', 'reduce',
           'partial', 'partialmethod', 'singledispatch', 'singledispatchmethod',
           'cached_property']


# `partial[int]` is a `types.GenericAlias` (the builtin containers' class
# subscription, with its arity check).
_class_getitem = list.__class_getitem__.__func__


################################################################################
### update_wrapper() and wraps() decorator
################################################################################

WRAPPER_ASSIGNMENTS = ('__module__', '__name__', '__qualname__', '__doc__',
                       '__annotate__', '__type_params__')
WRAPPER_UPDATES = ('__dict__',)


def update_wrapper(wrapper,
                   wrapped,
                   assigned=WRAPPER_ASSIGNMENTS,
                   updated=WRAPPER_UPDATES):
    for attr in assigned:
        try:
            value = getattr(wrapped, attr)
        except AttributeError:
            pass
        else:
            setattr(wrapper, attr, value)
    for attr in updated:
        getattr(wrapper, attr).update(getattr(wrapped, attr, {}))
    wrapper.__wrapped__ = wrapped
    return wrapper


def wraps(wrapped,
          assigned=WRAPPER_ASSIGNMENTS,
          updated=WRAPPER_UPDATES):
    return partial(update_wrapper, wrapped=wrapped,
                   assigned=assigned, updated=updated)


################################################################################
### total_ordering class decorator
################################################################################

def _gt_from_lt(self, other):
    op_result = type(self).__lt__(self, other)
    if op_result is NotImplemented:
        return op_result
    return not op_result and self != other


def _le_from_lt(self, other):
    op_result = type(self).__lt__(self, other)
    if op_result is NotImplemented:
        return op_result
    return op_result or self == other


def _ge_from_lt(self, other):
    op_result = type(self).__lt__(self, other)
    if op_result is NotImplemented:
        return op_result
    return not op_result


def _ge_from_le(self, other):
    op_result = type(self).__le__(self, other)
    if op_result is NotImplemented:
        return op_result
    return not op_result or self == other


def _lt_from_le(self, other):
    op_result = type(self).__le__(self, other)
    if op_result is NotImplemented:
        return op_result
    return op_result and self != other


def _gt_from_le(self, other):
    op_result = type(self).__le__(self, other)
    if op_result is NotImplemented:
        return op_result
    return not op_result


def _lt_from_gt(self, other):
    op_result = type(self).__gt__(self, other)
    if op_result is NotImplemented:
        return op_result
    return not op_result and self != other


def _ge_from_gt(self, other):
    op_result = type(self).__gt__(self, other)
    if op_result is NotImplemented:
        return op_result
    return op_result or self == other


def _le_from_gt(self, other):
    op_result = type(self).__gt__(self, other)
    if op_result is NotImplemented:
        return op_result
    return not op_result


def _le_from_ge(self, other):
    op_result = type(self).__ge__(self, other)
    if op_result is NotImplemented:
        return op_result
    return not op_result or self == other


def _gt_from_ge(self, other):
    op_result = type(self).__ge__(self, other)
    if op_result is NotImplemented:
        return op_result
    return op_result and self != other


def _lt_from_ge(self, other):
    op_result = type(self).__ge__(self, other)
    if op_result is NotImplemented:
        return op_result
    return not op_result


_convert = {
    '__lt__': [('__gt__', _gt_from_lt),
               ('__le__', _le_from_lt),
               ('__ge__', _ge_from_lt)],
    '__le__': [('__ge__', _ge_from_le),
               ('__lt__', _lt_from_le),
               ('__gt__', _gt_from_le)],
    '__gt__': [('__lt__', _lt_from_gt),
               ('__ge__', _ge_from_gt),
               ('__le__', _le_from_gt)],
    '__ge__': [('__le__', _le_from_ge),
               ('__gt__', _gt_from_ge),
               ('__lt__', _lt_from_ge)]
}


def _defines(cls, name):
    for c in cls.__mro__:
        if c is object:
            return False
        try:
            if name in c.__dict__:
                return True
        except AttributeError:
            pass
    return False


def total_ordering(cls):
    """Class decorator that fills in missing ordering methods"""
    roots = {op for op in _convert if _defines(cls, op)}
    if not roots:
        raise ValueError('must define at least one ordering operation: < > <= >=')
    root = max(roots)       # prefer __lt__ to __le__ to __gt__ to __ge__
    for opname, opfunc in _convert[root]:
        if opname not in roots:
            setattr(cls, opname, _named(opfunc, opname))
    return cls


def _named(f, name):
    def method(self, other):
        return f(self, other)
    method.__name__ = name
    method.__qualname__ = name
    return method


################################################################################
### cmp_to_key() function converter
################################################################################

def cmp_to_key(mycmp):
    """Convert a cmp= function into a key= function"""
    class K(object):
        def __init__(self, obj):
            self.obj = obj

        def __lt__(self, other):
            return mycmp(self.obj, other.obj) < 0

        def __gt__(self, other):
            return mycmp(self.obj, other.obj) > 0

        def __eq__(self, other):
            return mycmp(self.obj, other.obj) == 0

        def __le__(self, other):
            return mycmp(self.obj, other.obj) <= 0

        def __ge__(self, other):
            return mycmp(self.obj, other.obj) >= 0

        __hash__ = None
    return K


################################################################################
### reduce() sequence to a single item
################################################################################

_initial_missing = object()


def reduce(*args, **kwargs):
    """
    reduce(function, iterable, /[, initial]) -> value

    Apply a function of two arguments cumulatively to the items of an iterable,
    from left to right.
    """
    n = len(args) + len(kwargs)
    if n > 3:
        raise TypeError(f'reduce() takes at most 3 arguments ({n} given)')
    if len(args) < 2:
        raise TypeError(f'reduce() takes at least 2 positional arguments ({len(args)} given)')
    initial = _initial_missing
    if len(args) == 3:
        initial = args[2]
    for k in kwargs:
        if k != 'initial':
            raise TypeError(f"reduce() got an unexpected keyword argument '{k}'")
        initial = kwargs[k]
    function = args[0]
    try:
        it = iter(args[1])
    except TypeError:
        raise TypeError('reduce() arg 2 must support iteration') from None

    if initial is _initial_missing:
        try:
            value = next(it)
        except StopIteration:
            raise TypeError('reduce() of empty iterable with no initial value') from None
    else:
        value = initial

    while True:
        try:
            element = next(it)
        except StopIteration:
            return value
        value = function(value, element)


################################################################################
### partial() argument application
################################################################################

class partial:
    """New function with partial application of the given arguments
    and keywords.
    """

    def __new__(cls, *args, **keywords):
        if not args:
            raise TypeError("type 'partial' takes at least one argument")
        func = args[0]
        args = args[1:]
        if not callable(func):
            raise TypeError('the first argument must be callable')
        if isinstance(func, partial):
            args = func.args + args
            keywords = {**func.keywords, **keywords}
            func = func.func
        self = object.__new__(cls)
        self.func = func
        self.args = args
        self.keywords = keywords
        return self

    def __call__(self, *args, **keywords):
        keywords = {**self.keywords, **keywords}
        return self.func(*self.args, *args, **keywords)

    def __get__(self, obj, objtype=None):
        if obj is None:
            return self
        return _bind(self, obj)

    def __reduce__(self):
        return type(self), (self.func,), (self.func, self.args,
               self.keywords or None, None)

    def __setstate__(self, state):
        if not isinstance(state, tuple):
            raise TypeError("argument to __setstate__ must be a tuple")
        if len(state) != 4:
            raise TypeError(f"expected 4 items in state, got {len(state)}")
        func, args, kwds, namespace = state
        if (not callable(func) or not isinstance(args, tuple) or
           (kwds is not None and not isinstance(kwds, dict))):
            raise TypeError("invalid partial state")
        self.func = func
        self.args = tuple(args)
        self.keywords = {} if kwds is None else dict(kwds)

    def __repr__(self):
        if _repr_enter(self):
            return '...'
        try:
            args = [repr(self.func)]
            args.extend(repr(x) for x in self.args)
            args.extend(f'{k}={v!r}' for (k, v) in self.keywords.items())
            return f"functools.partial({', '.join(args)})"
        finally:
            _repr_leave(self)

    __mm_tpname__ = 'functools.partial'
    __class_getitem__ = classmethod(_class_getitem)


class _MethodMaker:
    def _m(self):
        pass


# A real bound method, as CPython's `__get__` returns: `obj.cached.cache_info()`
# and the wrapper's other attributes reach through it.
_bind = type(_MethodMaker()._m)


_repr_running = []


def _repr_enter(obj):
    for o in _repr_running:
        if o is obj:
            return True
    _repr_running.append(obj)
    return False


def _repr_leave(obj):
    for i in range(len(_repr_running) - 1, -1, -1):
        if _repr_running[i] is obj:
            del _repr_running[i]
            return


class partialmethod:
    """Method descriptor with partial application of the given arguments
    and keywords.
    """

    def __init__(self, func, /, *args, **keywords):
        if not callable(func) and not hasattr(func, '__get__'):
            raise TypeError(f'{func!r} is not callable or a descriptor')
        if isinstance(func, partialmethod):
            self.func = func.func
            self.args = func.args + args
            self.keywords = {**func.keywords, **keywords}
        else:
            self.func = func
            self.args = args
            self.keywords = keywords

    def __repr__(self):
        args = [repr(self.func)]
        args.extend(repr(x) for x in self.args)
        args.extend(f'{k}={v!r}' for (k, v) in self.keywords.items())
        return f"functools.partialmethod({', '.join(args)})"

    def _make_unbound_method(self):
        def _method(cls_or_self, /, *args, **keywords):
            keywords = {**self.keywords, **keywords}
            return self.func(cls_or_self, *self.args, *args, **keywords)
        _method.__isabstractmethod__ = self.__isabstractmethod__
        _method.__partialmethod__ = self
        return _method

    def __get__(self, obj, cls=None):
        get = getattr(self.func, '__get__', None)
        result = None
        if get is not None:
            new_func = get(obj, cls)
            if new_func is not self.func:
                # A `__get__` returning something new made the callable.
                result = partial(new_func, *self.args, **self.keywords)
                try:
                    result.__self__ = new_func.__self__
                except AttributeError:
                    pass
        if result is None:
            result = self._make_unbound_method().__get__(obj, cls)
        return result

    @property
    def __isabstractmethod__(self):
        return getattr(self.func, '__isabstractmethod__', False)

    __class_getitem__ = classmethod(_class_getitem)


################################################################################
### LRU Cache function decorator
################################################################################

_CacheInfo = namedtuple('CacheInfo', ['hits', 'misses', 'maxsize', 'currsize'])

_kwd_mark = object()


def _make_key(args, kwds, typed):
    key = args
    if kwds:
        key += (_kwd_mark,)
        for item in kwds.items():
            key += item
    if typed:
        key += tuple([type(v) for v in args])
        if kwds:
            key += tuple([type(v) for v in kwds.values()])
    elif len(key) == 1 and type(key[0]) in (int, str):
        return key[0]
    hash(key)
    return key


def lru_cache(maxsize=128, typed=False):
    """Least-recently-used cache decorator."""
    if isinstance(maxsize, int):
        if maxsize < 0:
            maxsize = 0
    elif callable(maxsize) and isinstance(typed, bool):
        user_function, maxsize = maxsize, 128
        return _lru_cache_wrapper(user_function, maxsize, typed)
    elif maxsize is not None:
        raise TypeError(
            'Expected first argument to be an integer, a callable, or None')

    def decorating_function(user_function):
        return _lru_cache_wrapper(user_function, maxsize, typed)

    return decorating_function


_MISSING = object()


def _lru_root():
    # The empty circular list: [prev, next, key, result].
    root = [None, None, None, None]
    root[0] = root[1] = root
    return root


class _lru_cache_wrapper:
    def __init__(self, user_function, maxsize, typed):
        if not callable(user_function):
            raise TypeError('the first argument must be callable')
        # The wrapped one's attributes first: its own private state (when it
        # is a cache too) is then replaced by this one's.
        update_wrapper(self, user_function)
        self._mm_func = user_function
        self._mm_maxsize = maxsize
        self._mm_typed = typed
        self._mm_cache = {}
        self._mm_root = _lru_root()
        self._mm_hits = 0
        self._mm_misses = 0

    def __call__(self, *args, **kwds):
        maxsize = self._mm_maxsize
        if maxsize == 0:
            self._mm_misses += 1
            return self._mm_func(*args, **kwds)
        key = _make_key(args, kwds, self._mm_typed)
        cache = self._mm_cache
        if maxsize is None:
            result = cache.get(key, _MISSING)
            if result is not _MISSING:
                self._mm_hits += 1
                return result
            self._mm_misses += 1
            result = self._mm_func(*args, **kwds)
            cache[key] = result
            return result
        # Bounded: _functools.c's dict of links on a circular list (oldest
        # first); a hit moves only the link, an eviction pops by key.
        link = cache.get(key)
        if link is not None:
            self._mm_hits += 1
            prev, nxt = link[0], link[1]
            prev[1] = nxt
            nxt[0] = prev
            root = self._mm_root
            last = root[0]
            last[1] = root[0] = link
            link[0] = last
            link[1] = root
            return link[3]
        self._mm_misses += 1
        result = self._mm_func(*args, **kwds)
        cache = self._mm_cache
        if key in cache:
            # A recursive call already stored this key.
            return result
        root = self._mm_root
        if len(cache) < maxsize or root[1] is root:
            last = root[0]
            link = [last, root, key, result]
            cache[key] = link
            last[1] = root[0] = link
            return result
        old = root[1]
        old[0][1] = old[1]
        old[1][0] = old[0]
        if cache.pop(old[2], _MISSING) is _MISSING:
            # The call removed the oldest key already: the link is an orphan.
            return result
        old[2] = key
        old[3] = result
        cache[key] = old
        last = root[0]
        last[1] = root[0] = old
        old[0] = last
        old[1] = root
        return result

    def __get__(self, obj, objtype=None):
        if obj is None:
            return self
        return _bind(self, obj)

    def cache_info(self, *args):
        if args:
            raise TypeError(f'_lru_cache_wrapper.cache_info() takes no arguments ({len(args)} given)')
        return _CacheInfo(self._mm_hits, self._mm_misses, self._mm_maxsize,
                          len(self._mm_cache))

    def cache_clear(self, *args):
        if args:
            raise TypeError(f'_lru_cache_wrapper.cache_clear() takes no arguments ({len(args)} given)')
        self._mm_cache = {}
        self._mm_root = _lru_root()
        self._mm_hits = self._mm_misses = 0

    def cache_parameters(self):
        return {'maxsize': self._mm_maxsize, 'typed': self._mm_typed}

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        return self

    __mm_tpname__ = 'functools._lru_cache_wrapper'


def cache(user_function, /):
    'Simple lightweight unbounded cache.  Sometimes called "memoize".'
    return lru_cache(maxsize=None)(user_function)


################################################################################
### singledispatch() - single-dispatch generic function decorator
################################################################################

def _find_impl(cls, registry):
    for t in cls.__mro__:
        if t in registry:
            return registry[t]
    return registry.get(object)


def _first_annotation(func):
    ann = getattr(func, '__annotations__', None)
    if not ann:
        raise TypeError(
            f"Invalid first argument to `register()`: {func!r}. "
            f"Use either `@register(some_class)` or plain `@register` "
            f"on an annotated function."
        )
    for name in ann:
        if name != 'return':
            return ann[name]


def singledispatch(func):
    """Single-dispatch generic function decorator."""
    registry = {}
    dispatch_cache = {}

    def dispatch(cls):
        impl = dispatch_cache.get(cls, _MISSING)
        if impl is _MISSING:
            impl = _find_impl(cls, registry)
            dispatch_cache[cls] = impl
        return impl

    def register(cls, func=None):
        if isinstance(cls, type):
            if func is None:
                return lambda f: register(cls, f)
        else:
            if func is not None:
                raise TypeError(
                    f"Invalid first argument to `register()`. "
                    f"{cls!r} is not a class or union type."
                )
            func = cls
            cls = _first_annotation(func)
            if not isinstance(cls, type):
                raise TypeError(
                    f"Invalid annotation for {func.__name__!r}. "
                    f"{cls!r} is not a class."
                )
        registry[cls] = func
        dispatch_cache.clear()
        return func

    def wrapper(*args, **kw):
        if not args:
            raise TypeError(f'{funcname} requires at least '
                            '1 positional argument')
        return dispatch(type(args[0]))(*args, **kw)

    funcname = getattr(func, '__name__', 'singledispatch function')
    registry[object] = func
    wrapper.register = register
    wrapper.dispatch = dispatch
    wrapper.registry = registry
    wrapper._clear_cache = dispatch_cache.clear
    update_wrapper(wrapper, func)
    return wrapper


class singledispatchmethod:
    """Single-dispatch generic method descriptor."""

    def __init__(self, func):
        if not callable(func) and not hasattr(func, '__get__'):
            raise TypeError(f'{func!r} is not callable or a descriptor')
        self.dispatcher = singledispatch(func)
        self.func = func

    def register(self, cls, method=None):
        return self.dispatcher.register(cls, func=method)

    def __get__(self, obj, cls=None):
        dispatcher = self.dispatcher
        func = self.func

        def _method(*args, **kwargs):
            if not args:
                raise TypeError(f'{getattr(func, "__name__", "singledispatchmethod method")} '
                                'requires at least 1 positional argument')
            method = dispatcher.dispatch(args[0].__class__)
            return method.__get__(obj, cls)(*args, **kwargs)
        update_wrapper(_method, func)
        return _method

    @property
    def __isabstractmethod__(self):
        return getattr(self.func, '__isabstractmethod__', False)


################################################################################
### cached_property() - property result cached as instance attribute
################################################################################

_NOT_FOUND = object()


class cached_property:
    def __init__(self, func):
        self.func = func
        self.attrname = None
        self.__doc__ = func.__doc__
        self.__module__ = func.__module__

    def __set_name__(self, owner, name):
        if self.attrname is None:
            self.attrname = name
        elif name != self.attrname:
            raise TypeError(
                "Cannot assign the same cached_property to two different names "
                f"({self.attrname!r} and {name!r})."
            )

    def __get__(self, instance, owner=None):
        if instance is None:
            return self
        if self.attrname is None:
            raise TypeError(
                "Cannot use cached_property instance without calling __set_name__ on it.")
        cache = instance.__dict__
        val = cache.get(self.attrname, _NOT_FOUND)
        if val is _NOT_FOUND:
            val = self.func(instance)
            cache[self.attrname] = val
        return val

    __class_getitem__ = classmethod(_class_getitem)
