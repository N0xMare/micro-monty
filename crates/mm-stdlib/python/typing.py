# typing: the runtime surface of CPython 3.14's Lib/typing.py (PSF license,
# see THIRD_PARTY.md) that annotations and common runtime code use. Type
# hints are runtime no-ops in micro-monty: special forms subscript to
# aliases that repr like CPython's, TypeVar/Generic/Protocol accept the
# usual spellings, and NamedTuple/TypedDict build real classes.

import collections
import functools

__all__ = [
    'Annotated', 'Any', 'Callable', 'ClassVar', 'Concatenate', 'Final',
    'ForwardRef', 'Generic', 'Literal', 'Optional', 'ParamSpec', 'Protocol',
    'Tuple', 'Type', 'TypeVar', 'TypeVarTuple', 'Union',
    'AbstractSet', 'ByteString', 'Container', 'ContextManager', 'Hashable',
    'ItemsView', 'Iterable', 'Iterator', 'KeysView', 'Mapping', 'MappingView',
    'MutableMapping', 'MutableSequence', 'MutableSet', 'Sequence', 'Sized',
    'ValuesView', 'Awaitable', 'AsyncIterator', 'AsyncIterable', 'Coroutine',
    'Collection', 'AsyncGenerator', 'AsyncContextManager', 'Reversible',
    'SupportsAbs', 'SupportsBytes', 'SupportsComplex', 'SupportsFloat',
    'SupportsIndex', 'SupportsInt', 'SupportsRound', 'ChainMap', 'Counter',
    'Deque', 'Dict', 'DefaultDict', 'List', 'OrderedDict', 'Set', 'FrozenSet',
    'NamedTuple', 'TypedDict', 'Generator', 'BinaryIO', 'IO', 'Match',
    'Pattern', 'TextIO', 'AnyStr', 'assert_type', 'assert_never', 'cast',
    'clear_overloads', 'dataclass_transform', 'final', 'get_args',
    'get_origin', 'get_overloads', 'get_protocol_members', 'get_type_hints',
    'is_protocol', 'is_typeddict', 'LiteralString', 'Never', 'NewType',
    'no_type_check', 'no_type_check_decorator', 'NoDefault', 'NoReturn',
    'NotRequired', 'overload', 'override', 'ParamSpecArgs', 'ParamSpecKwargs',
    'ReadOnly', 'Required', 'reveal_type', 'runtime_checkable', 'Self',
    'Text', 'TYPE_CHECKING', 'TypeAlias', 'TypeGuard', 'TypeIs',
    'TypeAliasType', 'Unpack',
]

TYPE_CHECKING = False


def _type_repr(obj):
    if obj is type(None) or obj is None:
        return 'None'
    if isinstance(obj, type):
        if obj.__module__ == 'builtins':
            return obj.__qualname__
        return f'{obj.__module__}.{obj.__qualname__}'
    if obj is ...:
        return '...'
    return repr(obj)


def _own_annotations(cls):
    try:
        return cls.__dict__.get('__annotations__', {})
    except AttributeError:
        return {}


def _args_tuple(params):
    if not isinstance(params, tuple):
        params = (params,)
    return tuple(type(None) if p is None else p for p in params)


class _GenericAlias:
    """A subscripted special form or generic class (`List[int]`)."""

    def __init__(self, origin, args, name=None):
        self.__origin__ = origin
        self.__args__ = args
        self._name = name
        self.__parameters__ = tuple(a for a in args if isinstance(a, TypeVar))

    def __repr__(self):
        if self._name is not None:
            head = 'typing.' + self._name
        else:
            head = _type_repr(self.__origin__)
        if not self.__args__:
            return head + '[()]'
        return head + '[' + ', '.join([_type_repr(a) for a in self.__args__]) + ']'

    def __eq__(self, other):
        if not isinstance(other, _GenericAlias):
            return NotImplemented
        return self.__origin__ == other.__origin__ and self.__args__ == other.__args__

    def __hash__(self):
        return hash((self.__origin__, self.__args__))

    def __getitem__(self, params):
        return _GenericAlias(self.__origin__, _args_tuple(params), self._name)

    def __call__(self, *args, **kwargs):
        o = self.__origin__
        if isinstance(o, _SpecialForm):
            return o(*args, **kwargs)
        if self._name in _NO_INSTANCES:
            raise TypeError(f'Type {self._name} cannot be instantiated; use {o.__name__}() instead')
        if not callable(o):
            raise TypeError(f'Type {self._name or _type_repr(o)} cannot be instantiated')
        return o(*args, **kwargs)

    def __mro_entries__(self, bases):
        o = self.__origin__
        if isinstance(o, _SpecialForm):
            o = _alias_origin(o)
        if o is None:
            raise TypeError(f'Cannot subclass {self!r}')
        if self._name is None:
            return (o,)
        return _alias_entries(self, o, bases)

    def __instancecheck__(self, obj):
        raise TypeError('Subscripted generics cannot be used with class and instance checks')

    def __subclasscheck__(self, cls):
        raise TypeError('Subscripted generics cannot be used with class and instance checks')


# Aliases that refuse to be called (CPython's `inst=False`).
_NO_INSTANCES = frozenset(('List', 'Tuple', 'Dict', 'Set', 'FrozenSet', 'Type'))

# Aliases whose class lives outside `collections.abc`: (module, name).
_ORIGIN_ELSEWHERE = {
    'Pattern': ('re', 'Pattern'),
    'Match': ('re', 'Match'),
    'ContextManager': ('contextlib', 'AbstractContextManager'),
    'AsyncContextManager': ('contextlib', 'AbstractAsyncContextManager'),
}


def _alias_origin(form):
    # The class a typing alias such as `Mapping` stands for, looked up in
    # `collections.abc` (or `re`, `contextlib`) only when one is subclassed
    # or used with isinstance()/issubclass().
    o = form.__origin__
    if o is None:
        where = _ORIGIN_ELSEWHERE.get(form._name)
        if where is None:
            import collections.abc
            o = getattr(collections.abc, form._name, None)
        elif where[0] == 're':
            import re
            o = getattr(re, where[1], None)
        else:
            import contextlib
            o = getattr(contextlib, where[1], None)
    return o if isinstance(o, type) else None


def _alias_entries(alias, origin, bases):
    # CPython's `_BaseGenericAlias.__mro_entries__`: the origin, then
    # `Generic` unless a later base brings it.
    res = []
    if origin not in bases:
        res.append(origin)
    i = bases.index(alias)
    for b in bases[i + 1:]:
        if isinstance(b, _GenericAlias) or (isinstance(b, type) and issubclass(b, Generic)):
            break
    else:
        res.append(Generic)
    return tuple(res)


class _SpecialForm:
    """A typing marker (`Any`, `Optional`, `ClassVar`, `List`, ...)."""

    def __init__(self, name, origin=None, getitem=None):
        self._name = name
        self.__origin__ = origin
        self._getitem = getitem

    def __repr__(self):
        return 'typing.' + self._name

    def __reduce__(self):
        return self._name

    def __call__(self, *args, **kwds):
        # CPython's aliases with `inst=False` refuse; the others (`Deque`,
        # `Counter`, ...) build their origin; the markers refuse.
        o = _alias_origin(self)
        if o is None:
            if self._name == 'Any':
                raise TypeError('Any cannot be instantiated')
            raise TypeError(f"Cannot instantiate {self!r}")
        if self._name in _NO_INSTANCES:
            raise TypeError(f'Type {self._name} cannot be instantiated; use {o.__name__}() instead')
        return o(*args, **kwds)

    def __mro_entries__(self, bases):
        o = _alias_origin(self)
        if o is None:
            raise TypeError(f'Cannot subclass {self!r}')
        return _alias_entries(self, o, bases)

    # `isinstance(x, List)`, `issubclass(C, Mapping)`: CPython's
    # `_SpecialGenericAlias` checks against the origin class; the markers
    # (`Any`, `Optional`, ...) refuse.
    def __instancecheck__(self, obj):
        o = _alias_origin(self)
        if o is None:
            raise TypeError(f'{self} cannot be used with isinstance()')
        return isinstance(obj, o)

    def __subclasscheck__(self, cls):
        o = _alias_origin(self)
        if o is None:
            raise TypeError(f'{self} cannot be used with issubclass()')
        if isinstance(cls, _SpecialForm):
            c = _alias_origin(cls)
            if c is not None:
                cls = c
        return issubclass(cls, o)

    def __or__(self, other):
        return Union[self, other]

    def __ror__(self, other):
        return Union[other, self]

    def __getitem__(self, parameters):
        if self._getitem is not None:
            return self._getitem(self, parameters)
        if self._name in ('Any', 'NoReturn', 'Never', 'Self', 'LiteralString', 'TypeAlias'):
            raise TypeError(f"{self} is not subscriptable")
        origin = self if self.__origin__ is None else self.__origin__
        return _GenericAlias(origin, _args_tuple(parameters), self._name)


def _optional(self, parameters):
    if type(parameters) is tuple:
        raise TypeError(f'typing.Optional requires a single type. Got {parameters!r}.')
    return Union[parameters, None]


def _alias(name, origin):
    return _SpecialForm(name, origin)


# `int | str` is a `typing.Union` (as in CPython 3.14, `types.UnionType` is
# `typing.Union`); `Union[X, Y]` builds the same.
Union = type(int | str)
_UnionAlias = Union
_TypesAlias = type(list[int])


Any = _SpecialForm('Any')
NoReturn = _SpecialForm('NoReturn')
Never = _SpecialForm('Never')
Self = _SpecialForm('Self')
LiteralString = _SpecialForm('LiteralString')
TypeAlias = _SpecialForm('TypeAlias')
ClassVar = _SpecialForm('ClassVar')
Final = _SpecialForm('Final')
Optional = _SpecialForm('Optional', getitem=_optional)
Literal = _SpecialForm('Literal')
Annotated = _SpecialForm('Annotated')
Concatenate = _SpecialForm('Concatenate')
TypeGuard = _SpecialForm('TypeGuard')
TypeIs = _SpecialForm('TypeIs')
Required = _SpecialForm('Required')
NotRequired = _SpecialForm('NotRequired')
ReadOnly = _SpecialForm('ReadOnly')
Unpack = _SpecialForm('Unpack')
Callable = _SpecialForm('Callable')

List = _alias('List', list)
Dict = _alias('Dict', dict)
Set = _alias('Set', set)
FrozenSet = _alias('FrozenSet', frozenset)
Tuple = _alias('Tuple', tuple)
Type = _alias('Type', type)
Deque = _alias('Deque', collections.deque)
DefaultDict = _alias('DefaultDict', collections.defaultdict)
OrderedDict = _alias('OrderedDict', collections.OrderedDict)
Counter = _alias('Counter', collections.Counter)
ChainMap = _alias('ChainMap', collections.ChainMap)
Text = str
ByteString = _alias('ByteString', bytes)

Hashable = _alias('Hashable', None)
Awaitable = _alias('Awaitable', None)
Coroutine = _alias('Coroutine', None)
AsyncIterable = _alias('AsyncIterable', None)
AsyncIterator = _alias('AsyncIterator', None)
Iterable = _alias('Iterable', None)
Iterator = _alias('Iterator', None)
Reversible = _alias('Reversible', None)
Sized = _alias('Sized', None)
Container = _alias('Container', None)
Collection = _alias('Collection', None)
AbstractSet = _alias('AbstractSet', None)
MutableSet = _alias('MutableSet', None)
Mapping = _alias('Mapping', None)
MutableMapping = _alias('MutableMapping', None)
Sequence = _alias('Sequence', None)
MutableSequence = _alias('MutableSequence', None)
MappingView = _alias('MappingView', None)
KeysView = _alias('KeysView', None)
ItemsView = _alias('ItemsView', None)
ValuesView = _alias('ValuesView', None)
Generator = _alias('Generator', None)
AsyncGenerator = _alias('AsyncGenerator', None)
ContextManager = _alias('ContextManager', None)
AsyncContextManager = _alias('AsyncContextManager', None)
Pattern = _alias('Pattern', None)
Match = _alias('Match', None)
IO = _alias('IO', None)
TextIO = _alias('TextIO', None)
BinaryIO = _alias('BinaryIO', None)
SupportsAbs = _alias('SupportsAbs', None)
SupportsBytes = _alias('SupportsBytes', None)
SupportsComplex = _alias('SupportsComplex', None)
SupportsFloat = _alias('SupportsFloat', None)
SupportsIndex = _alias('SupportsIndex', None)
SupportsInt = _alias('SupportsInt', None)
SupportsRound = _alias('SupportsRound', None)


class TypeVar:
    """Type variable."""

    def __init__(self, name, *constraints, bound=None, covariant=False,
                 contravariant=False, infer_variance=False, default=None):
        self.__name__ = name
        if covariant and contravariant:
            raise ValueError("Bivariant type variables are not supported.")
        if constraints and bound is not None:
            raise TypeError("Constraints cannot be combined with bound=...")
        if len(constraints) == 1:
            raise TypeError("A single constraint is not allowed")
        self.__constraints__ = constraints
        self.__bound__ = bound
        self.__covariant__ = bool(covariant)
        self.__contravariant__ = bool(contravariant)
        self.__infer_variance__ = bool(infer_variance)
        self.__default__ = default

    def __repr__(self):
        if self.__infer_variance__:
            prefix = ''
        elif self.__covariant__:
            prefix = '+'
        elif self.__contravariant__:
            prefix = '-'
        else:
            prefix = '~'
        return prefix + self.__name__

    def __or__(self, other):
        return Union[self, other]

    def __ror__(self, other):
        return Union[other, self]


class ParamSpec:
    def __init__(self, name, *, bound=None, covariant=False,
                 contravariant=False, infer_variance=False, default=None):
        self.__name__ = name
        self.args = ParamSpecArgs(self)
        self.kwargs = ParamSpecKwargs(self)

    def __repr__(self):
        return '~' + self.__name__


class ParamSpecArgs:
    def __init__(self, origin):
        self.__origin__ = origin

    def __repr__(self):
        return f"{self.__origin__.__name__}.args"


class ParamSpecKwargs:
    def __init__(self, origin):
        self.__origin__ = origin

    def __repr__(self):
        return f"{self.__origin__.__name__}.kwargs"


class TypeVarTuple:
    def __init__(self, name, *, default=None):
        self.__name__ = name

    def __repr__(self):
        return self.__name__

    def __iter__(self):
        yield Unpack[self]


AnyStr = TypeVar('AnyStr', bytes, str)

NoDefault = _SpecialForm('NoDefault')


class Generic:
    """Abstract base class for generic types."""

    def __class_getitem__(cls, params):
        # `Generic[T]` and `Protocol[T]` are written as bases; micro-monty has
        # no `__mro_entries__`, so they stand for the class itself.
        if cls is Generic or cls is Protocol:
            return cls
        return _GenericAlias(cls, _args_tuple(params))


class Protocol(Generic):
    """Base class for protocol classes."""
    _is_protocol = True

    def __init_subclass__(cls, *args, **kwargs):
        super().__init_subclass__(*args, **kwargs)
        if '_is_protocol' not in cls.__dict__:
            cls._is_protocol = any(b is Protocol for b in cls.__bases__)


def runtime_checkable(cls):
    return cls


def is_protocol(tp):
    return isinstance(tp, type) and getattr(tp, '_is_protocol', False) and tp is not Protocol


def get_protocol_members(tp):
    if not is_protocol(tp):
        raise TypeError(f'{tp!r} is not a Protocol')
    return set(n for n in tp.__dict__ if not n.startswith('_'))


class ForwardRef:
    def __init__(self, arg, is_argument=True, module=None, *, is_class=False):
        if not isinstance(arg, str):
            raise TypeError(f"Forward reference must be a string -- got {arg!r}")
        self.__forward_arg__ = arg
        self.__forward_module__ = module

    def __eq__(self, other):
        if not isinstance(other, ForwardRef):
            return NotImplemented
        return self.__forward_arg__ == other.__forward_arg__

    def __hash__(self):
        return hash(self.__forward_arg__)

    def __repr__(self):
        return f'ForwardRef({self.__forward_arg__!r})'


class NewType:
    """NewType creates simple unique types with almost zero runtime overhead."""

    def __init__(self, name, tp):
        self.__qualname__ = name
        if '.' in name:
            name = name.rpartition('.')[-1]
        self.__name__ = name
        self.__supertype__ = tp
        self.__module__ = '__main__'

    def __repr__(self):
        return f'{self.__module__}.{self.__qualname__}'

    def __call__(self, x):
        return x

    def __or__(self, other):
        return Union[self, other]

    def __ror__(self, other):
        return Union[other, self]


class TypeAliasType:
    def __init__(self, name, value, *, type_params=()):
        self.__name__ = name
        self.__value__ = value
        self.__type_params__ = type_params

    def __repr__(self):
        return self.__name__


def cast(typ, val):
    """Cast a value to a type (returns the value unchanged)."""
    return val


def assert_type(val, typ, /):
    return val


def reveal_type(obj, /):
    print(f"Runtime type is {type(obj).__name__!r}")
    return obj


def assert_never(arg, /):
    value = repr(arg)
    if len(value) > 100:
        value = value[:100] + '...'
    raise AssertionError(f"Expected code to be unreachable, but got: {value}")


_overload_registry = {}


def _overload_dummy(*args, **kwds):
    raise NotImplementedError(
        "You should not call an overloaded function. "
        "A series of @typing.overload-decorated functions "
        "outside a stub module should always be followed "
        "by an implementation that is not @typing.overload-decorated.")


def overload(func):
    key = getattr(func, '__qualname__', None)
    if key is not None:
        _overload_registry.setdefault(key, []).append(func)
    return _overload_dummy


def get_overloads(func):
    return list(_overload_registry.get(getattr(func, '__qualname__', None), []))


def clear_overloads():
    _overload_registry.clear()


def final(f):
    try:
        f.__final__ = True
    except (AttributeError, TypeError):
        pass
    return f


def override(method, /):
    try:
        method.__override__ = True
    except (AttributeError, TypeError):
        pass
    return method


def no_type_check(arg):
    return arg


def no_type_check_decorator(decorator):
    return decorator


def dataclass_transform(*, eq_default=True, order_default=False,
                        kw_only_default=False, frozen_default=False,
                        field_specifiers=(), **kwargs):
    def decorator(cls_or_fn):
        cls_or_fn.__dataclass_transform__ = {
            "eq_default": eq_default,
            "order_default": order_default,
            "kw_only_default": kw_only_default,
            "frozen_default": frozen_default,
            "field_specifiers": field_specifiers,
            "kwargs": kwargs,
        }
        return cls_or_fn
    return decorator


def get_origin(tp):
    """Get the unsubscripted version of a type."""
    if isinstance(tp, (_GenericAlias, _TypesAlias, Union)):
        return tp.__origin__
    if tp is Generic:
        return Generic
    return None


def get_args(tp):
    """Get type arguments with all substitutions performed."""
    if isinstance(tp, (_GenericAlias, _TypesAlias, Union)):
        return tp.__args__
    return ()


def get_type_hints(obj, globalns=None, localns=None, include_extras=False):
    """Return the annotations of a class (micro-monty keeps them as
    strings; they are not evaluated)."""
    hints = {}
    if isinstance(obj, type):
        for base in reversed(obj.__mro__):
            ann = _own_annotations(base)
            for name, value in ann.items():
                hints[name] = value
        return hints
    return dict(getattr(obj, '__annotations__', {}) or {})


def is_typeddict(tp):
    return isinstance(tp, type) and getattr(tp, '_is_typeddict', False)


# --------------------------------------------------------------- NamedTuple

def _namedtuple_from(cls_name, fields, defaults, module):
    nt = collections.namedtuple(cls_name, [n for n, _ in fields],
                                defaults=defaults, module=module)
    nt.__annotations__ = dict(fields)
    return nt


class NamedTuple(tuple):
    """Typed version of namedtuple.

    class Employee(NamedTuple):
        name: str
        id: int = 3
    """

    def __new__(cls, typename=None, fields=None, /, **kwargs):
        if cls is not NamedTuple:
            return tuple.__new__(cls)
        if fields is None:
            fields = list(kwargs.items())
        elif kwargs:
            raise TypeError("Either list of fields or keywords"
                            " can be provided to NamedTuple, not both")
        return _namedtuple_from(typename, [(n, t) for n, t in fields], None, '__main__')

    def __init_subclass__(cls, **kwargs):
        ann = cls.__dict__.get('__annotations__', {})
        names = list(ann)
        defaults = []
        for name in names:
            if name in cls.__dict__:
                defaults.append(cls.__dict__[name])
            elif defaults:
                default_names = ', '.join(names[len(names) - len(defaults) - 1:])
                raise TypeError(f"Non-default namedtuple field {name} "
                                f"cannot follow default field"
                                f"{'s' if len(defaults) > 1 else ''} "
                                f"{default_names}")
        nt = collections.namedtuple(cls.__name__, names, defaults=defaults,
                                    module=cls.__module__)
        for key in ('__new__', '__repr__', '_make', '_replace', '_asdict',
                    '__getnewargs__', '_fields', '_field_defaults',
                    '__match_args__', '__replace__'):
            if key in nt.__dict__ and (key not in cls.__dict__ or key in ('__new__', '_fields')):
                setattr(cls, key, nt.__dict__[key])
        for name in names:
            setattr(cls, name, nt.__dict__[name])
        cls.__qualname__ = cls.__qualname__


# ---------------------------------------------------------------- TypedDict

class TypedDict(dict):
    """A simple typed namespace. At runtime it is a plain dict."""

    def __new__(cls, *args, total=True, **kwargs):
        if cls is TypedDict:
            typename, fields = args[0], (args[1] if len(args) > 1 else {})
            ns = {'__annotations__': dict(fields), '__total__': total}
            return type(typename, (TypedDict,), ns)
        return dict(*args, **kwargs)

    def __init_subclass__(cls, total=True, **kwargs):
        cls.__total__ = total
        cls._is_typeddict = True
        ann = {}
        for base in reversed(cls.__mro__):
            ann.update(_own_annotations(base))
        keys = set(ann)
        if total:
            cls.__required_keys__ = keys
            cls.__optional_keys__ = set()
        else:
            cls.__required_keys__ = set()
            cls.__optional_keys__ = keys
