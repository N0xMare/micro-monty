# abc, after CPython 3.14's Lib/abc.py and Lib/_py_abc.py (PSF license, see
# THIRD_PARTY.md). Registries and caches are plain sets (no weak
# references); ABCMeta.__call__ refuses to instantiate a class that still
# has abstract methods, with object.__new__'s message. `register` also
# hands an ABC's `__abc_tpflags__` (sequence/mapping for `match`) to a
# registered class, as CPython's C implementation does.


def abstractmethod(funcobj):
    """A decorator indicating abstract methods."""
    funcobj.__isabstractmethod__ = True
    return funcobj


# The deprecated decorator classes, as functions (builtin descriptor types
# cannot be subclassed here).
def abstractclassmethod(callable):
    callable.__isabstractmethod__ = True
    return classmethod(callable)


def abstractstaticmethod(callable):
    callable.__isabstractmethod__ = True
    return staticmethod(callable)


def abstractproperty(fget=None, fset=None, fdel=None, doc=None):
    if fget is not None:
        fget.__isabstractmethod__ = True
    return property(fget, fset, fdel, doc)


def _is_abstract(value):
    if getattr(value, '__isabstractmethod__', False):
        return True
    for attr in ('fget', 'fset', 'fdel', '__func__'):
        inner = getattr(value, attr, None)
        if inner is not None and getattr(inner, '__isabstractmethod__', False):
            return True
    return False


def _abstracts(cls, own):
    abstracts = {name for name, value in own if _is_abstract(value)}
    for base in cls.__bases__:
        for name in getattr(base, '__abstractmethods__', ()):
            if _is_abstract(getattr(cls, name, None)):
                abstracts.add(name)
    return frozenset(abstracts)


class ABCMeta(type):
    """Metaclass for defining Abstract Base Classes (ABCs)."""

    _abc_invalidation_counter = 0

    def __new__(mcls, name, bases, namespace, /, **kwargs):
        cls = super().__new__(mcls, name, bases, namespace, **kwargs)
        cls.__abstractmethods__ = _abstracts(cls, namespace.items())
        cls._abc_registry = set()
        cls._abc_subclasses = []
        cls._abc_cache = set()
        cls._abc_negative_cache = set()
        cls._abc_negative_cache_version = ABCMeta._abc_invalidation_counter
        for base in bases:
            if isinstance(base, ABCMeta):
                base._abc_subclasses.append(cls)
        return cls

    def __call__(cls, /, *args, **kwargs):
        missing = cls.__abstractmethods__
        if missing:
            names = ', '.join(f"'{n}'" for n in sorted(missing))
            s = '' if len(missing) == 1 else 's'
            raise TypeError(f"Can't instantiate abstract class {cls.__name__} "
                            f"without an implementation for abstract method{s} {names}")
        return super().__call__(*args, **kwargs)

    def register(cls, subclass):
        """Register a virtual subclass of an ABC."""
        if not isinstance(subclass, type):
            raise TypeError('Can only register classes')
        if issubclass(subclass, cls):
            return subclass
        if issubclass(cls, subclass):
            raise RuntimeError('Refusing to create an inheritance cycle')
        cls._abc_registry.add(subclass)
        ABCMeta._abc_invalidation_counter += 1
        flags = getattr(cls, '__abc_tpflags__', 0)
        if flags and not getattr(subclass, '__abc_tpflags__', 0):
            try:
                type.__setattr__(subclass, '__abc_tpflags__', flags)
            except (TypeError, AttributeError):
                pass
        return subclass

    def __instancecheck__(cls, instance):
        return cls.__subclasscheck__(type(instance))

    def __subclasscheck__(cls, subclass):
        if not isinstance(subclass, type):
            raise TypeError('issubclass() arg 1 must be a class')
        if subclass in cls._abc_cache:
            return True
        if cls._abc_negative_cache_version < ABCMeta._abc_invalidation_counter:
            cls._abc_negative_cache = set()
            cls._abc_negative_cache_version = ABCMeta._abc_invalidation_counter
        elif subclass in cls._abc_negative_cache:
            return False
        hook = getattr(cls, '__subclasshook__', None)
        ok = NotImplemented if hook is None else hook(subclass)
        if ok is not NotImplemented:
            if ok:
                cls._abc_cache.add(subclass)
            else:
                cls._abc_negative_cache.add(subclass)
            return ok
        if cls in subclass.__mro__:
            cls._abc_cache.add(subclass)
            return True
        for scls in list(cls._abc_registry) + cls._abc_subclasses:
            if issubclass(subclass, scls):
                cls._abc_cache.add(subclass)
                return True
        cls._abc_negative_cache.add(subclass)
        return False

    def _abc_registry_clear(cls):
        cls._abc_registry = set()

    def _abc_caches_clear(cls):
        cls._abc_cache = set()
        cls._abc_negative_cache = set()


def update_abstractmethods(cls):
    """Recalculate the set of abstract methods of an abstract class."""
    if not hasattr(cls, '__abstractmethods__'):
        return cls
    cls.__abstractmethods__ = _abstracts(cls, list(cls.__dict__.items()))
    return cls


class ABC(metaclass=ABCMeta):
    """Helper class that provides a standard way to create an ABC using
    inheritance."""


def get_cache_token():
    return ABCMeta._abc_invalidation_counter
