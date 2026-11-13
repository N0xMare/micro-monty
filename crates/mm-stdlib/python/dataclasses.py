# dataclasses, adapted from CPython 3.14's Lib/dataclasses.py (PSF license,
# see THIRD_PARTY.md). CPython generates the methods as source and `exec`s
# it; here they are closures over the field lists taken at decoration, and
# `__init__`'s signature comes from `_mm.signature`, so binding errors read
# as CPython's. Annotations are strings (micro-monty stores class
# annotations stringized), so ClassVar/InitVar/KW_ONLY are recognized by
# name, as CPython does for string annotations.

import copy
from _mm import signature as _signature

__all__ = ['dataclass',
           'field',
           'Field',
           'FrozenInstanceError',
           'InitVar',
           'KW_ONLY',
           'MISSING',

           # Helper functions.
           'fields',
           'asdict',
           'astuple',
           'make_dataclass',
           'replace',
           'is_dataclass',
           ]


class FrozenInstanceError(AttributeError): pass


class _HAS_DEFAULT_FACTORY_CLASS:
    def __repr__(self):
        return '<factory>'


_HAS_DEFAULT_FACTORY = _HAS_DEFAULT_FACTORY_CLASS()


class _MISSING_TYPE:
    pass


MISSING = _MISSING_TYPE()


class _KW_ONLY_TYPE:
    pass


KW_ONLY = _KW_ONLY_TYPE()


class _MappingProxy:
    # `types.MappingProxyType`: a read-only view of a mapping.
    __mm_tpname__ = 'mappingproxy'

    def __init__(self, mapping):
        self._mapping = mapping

    def __getitem__(self, key):
        return self._mapping[key]

    def __len__(self):
        return len(self._mapping)

    def __iter__(self):
        return iter(self._mapping)

    def __contains__(self, key):
        return key in self._mapping

    def __eq__(self, other):
        return self._mapping == other

    def get(self, key, default=None):
        return self._mapping.get(key, default)

    def keys(self):
        return self._mapping.keys()

    def values(self):
        return self._mapping.values()

    def items(self):
        return self._mapping.items()

    def copy(self):
        return self._mapping.copy()

    def __repr__(self):
        return f'mappingproxy({self._mapping!r})'


_EMPTY_METADATA = _MappingProxy({})


class _FIELD_BASE:
    def __init__(self, name):
        self.name = name

    def __repr__(self):
        return self.name


_FIELD = _FIELD_BASE('_FIELD')
_FIELD_CLASSVAR = _FIELD_BASE('_FIELD_CLASSVAR')
_FIELD_INITVAR = _FIELD_BASE('_FIELD_INITVAR')

_FIELDS = '__dataclass_fields__'
_PARAMS = '__dataclass_params__'
_POST_INIT_NAME = '__post_init__'

_ATOMIC_TYPES = {type(None), bool, int, float, str, bytes, type(...),
                 type(NotImplemented), type(len), type(lambda: None), type,
                 range, property}


class InitVar:
    def __init__(self, type):
        self.type = type

    def __repr__(self):
        if isinstance(self.type, type):
            type_name = self.type.__name__
        else:
            type_name = repr(self.type)
        return f'dataclasses.InitVar[{type_name}]'

    def __class_getitem__(cls, type):
        return InitVar(type)


class Field:
    def __init__(self, default, default_factory, init, repr, hash, compare,
                 metadata, kw_only, doc):
        self.name = None
        self.type = None
        self.default = default
        self.default_factory = default_factory
        self.init = init
        self.repr = repr
        self.hash = hash
        self.compare = compare
        self.metadata = (_EMPTY_METADATA
                         if metadata is None else
                         _MappingProxy(metadata))
        self.kw_only = kw_only
        self.doc = doc
        self._field_type = None

    def __repr__(self):
        if _repr_enter(self):
            return '...'
        try:
            return ('Field('
                    f'name={self.name!r},'
                    f'type={self.type!r},'
                    f'default={self.default!r},'
                    f'default_factory={self.default_factory!r},'
                    f'init={self.init!r},'
                    f'repr={self.repr!r},'
                    f'hash={self.hash!r},'
                    f'compare={self.compare!r},'
                    f'metadata={self.metadata!r},'
                    f'kw_only={self.kw_only!r},'
                    f'doc={self.doc!r},'
                    f'_field_type={self._field_type}'
                    ')')
        finally:
            _repr_leave(self)

    def __set_name__(self, owner, name):
        func = getattr(type(self.default), '__set_name__', None)
        if func:
            func(self.default, owner, name)

    def __class_getitem__(cls, item):
        return cls


class _DataclassParams:
    def __init__(self,
                 init, repr, eq, order, unsafe_hash, frozen,
                 match_args, kw_only, slots, weakref_slot):
        self.init = init
        self.repr = repr
        self.eq = eq
        self.order = order
        self.unsafe_hash = unsafe_hash
        self.frozen = frozen
        self.match_args = match_args
        self.kw_only = kw_only
        self.slots = slots
        self.weakref_slot = weakref_slot

    def __repr__(self):
        return ('_DataclassParams('
                f'init={self.init!r},'
                f'repr={self.repr!r},'
                f'eq={self.eq!r},'
                f'order={self.order!r},'
                f'unsafe_hash={self.unsafe_hash!r},'
                f'frozen={self.frozen!r},'
                f'match_args={self.match_args!r},'
                f'kw_only={self.kw_only!r},'
                f'slots={self.slots!r},'
                f'weakref_slot={self.weakref_slot!r}'
                ')')


def field(*, default=MISSING, default_factory=MISSING, init=True, repr=True,
          hash=None, compare=True, metadata=None, kw_only=MISSING, doc=None):
    """Return an object to identify dataclass fields."""
    if default is not MISSING and default_factory is not MISSING:
        raise ValueError('cannot specify both default and default_factory')
    return Field(default, default_factory, init, repr, hash, compare,
                 metadata, kw_only, doc)


def _fields_in_init_order(fields):
    return (tuple(f for f in fields if f.init and not f.kw_only),
            tuple(f for f in fields if f.init and f.kw_only)
            )


# Objects whose repr is running (reprlib.recursive_repr's guard).
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


def _named(fn, cls, name):
    fn.__name__ = name
    fn.__qualname__ = f'{cls.__qualname__}.{name}'
    return fn


def _init_fn(cls, fields, std_fields, kw_only_fields, frozen, has_post_init,
             self_name):
    seen_default = None
    for f in std_fields:
        if f.init:
            if not (f.default is MISSING and f.default_factory is MISSING):
                seen_default = f
            elif seen_default:
                raise TypeError(f'non-default argument {f.name!r} '
                                f'follows default argument {seen_default.name!r}')

    def default_of(f):
        if f.default is not MISSING:
            return f.default
        if f.default_factory is not MISSING:
            return _HAS_DEFAULT_FACTORY
        return MISSING

    params = [self_name] + [f.name for f in std_fields]
    kwonly = [f.name for f in kw_only_fields]
    defaults = []
    for f in std_fields:
        d = default_of(f)
        if d is MISSING:
            defaults = []
        else:
            defaults.append(d)
    kwdefaults = {}
    for f in kw_only_fields:
        d = default_of(f)
        if d is not MISSING:
            kwdefaults[f.name] = d
    order = [f.name for f in std_fields] + kwonly

    # What the body does per field, decided now: (name, is_initvar,
    # in_params, factory).
    plan = []
    for f in fields:
        factory = f.default_factory
        if factory is not MISSING:
            plan.append((f.name, f._field_type is _FIELD_INITVAR, f.init, factory))
        elif f.init:
            plan.append((f.name, f._field_type is _FIELD_INITVAR, True, None))
    initvars = [f.name for f in fields if f._field_type is _FIELD_INITVAR]
    setter = object.__setattr__

    def __init__(self, *values):
        given = dict(zip(order, values))
        for name, initvar, in_params, factory in plan:
            if in_params:
                value = given[name]
                if factory is not None and value is _HAS_DEFAULT_FACTORY:
                    value = factory()
                    given[name] = value
            else:
                value = factory()
            if initvar:
                continue
            if frozen:
                setter(self, name, value)
            else:
                setattr(self, name, value)
        if has_post_init:
            self.__post_init__(*[given[n] for n in initvars])

    return _signature('__init__', f'{cls.__qualname__}.__init__', params,
                         kwonly, tuple(defaults) if defaults else None,
                         kwdefaults or None, __init__)


def _repr_fn(cls, flds):
    names = [f.name for f in flds]

    def __repr__(self):
        if _repr_enter(self):
            return '...'
        try:
            parts = [f'{n}={getattr(self, n)!r}' for n in names]
        finally:
            _repr_leave(self)
        return f"{type(self).__qualname__}({', '.join(parts)})"
    return _named(__repr__, cls, '__repr__')


def _eq_fn(cls, flds):
    names = [f.name for f in flds]

    def __eq__(self, other):
        if self is other:
            return True
        if type(other) is type(self):
            result = True
            for n in names:
                result = getattr(self, n) == getattr(other, n)
                if not result:
                    return result
            return result
        return NotImplemented
    return _named(__eq__, cls, '__eq__')


def _order_fn(cls, flds, name):
    names = [f.name for f in flds]

    def tup(obj):
        return tuple([getattr(obj, n) for n in names])

    if name == '__lt__':
        def op(self, other):
            if type(other) is type(self):
                return tup(self) < tup(other)
            return NotImplemented
    elif name == '__le__':
        def op(self, other):
            if type(other) is type(self):
                return tup(self) <= tup(other)
            return NotImplemented
    elif name == '__gt__':
        def op(self, other):
            if type(other) is type(self):
                return tup(self) > tup(other)
            return NotImplemented
    else:
        def op(self, other):
            if type(other) is type(self):
                return tup(self) >= tup(other)
            return NotImplemented
    return _named(op, cls, name)


def _hash_fn(cls, flds):
    names = [f.name for f in flds]

    def __hash__(self):
        return hash(tuple([getattr(self, n) for n in names]))
    return _named(__hash__, cls, '__hash__')


def _frozen_fns(cls, fields):
    names = {f.name for f in fields}

    def __setattr__(self, name, value):
        if type(self) is cls or name in names:
            raise FrozenInstanceError(f"cannot assign to field {name!r}")
        super(cls, self).__setattr__(name, value)

    def __delattr__(self, name):
        if type(self) is cls or name in names:
            raise FrozenInstanceError(f"cannot delete field {name!r}")
        super(cls, self).__delattr__(name)
    return (_named(__setattr__, cls, '__setattr__'),
            _named(__delattr__, cls, '__delattr__'))


def _annotation_name(annotation):
    # `ident` or `module.ident`, followed by anything (CPython's
    # _MODULE_IDENTIFIER_RE): the identifier, else None.
    s = annotation.strip()
    end = 0
    while end < len(s) and (s[end].isalnum() or s[end] == '_'):
        end += 1
    if end == 0:
        return None
    first = s[:end]
    rest = s[end:].lstrip()
    if rest.startswith('.'):
        rest = rest[1:].lstrip()
        end = 0
        while end < len(rest) and (rest[end].isalnum() or rest[end] == '_'):
            end += 1
        if end == 0:
            return first
        return rest[:end]
    return first


def _is_classvar(a_type):
    if isinstance(a_type, str):
        return _annotation_name(a_type) == 'ClassVar'
    typing = _typing()
    if typing is None:
        return False
    return a_type is typing.ClassVar or typing.get_origin(a_type) is typing.ClassVar


def _is_initvar(a_type):
    if isinstance(a_type, str):
        return _annotation_name(a_type) == 'InitVar'
    return a_type is InitVar or type(a_type) is InitVar


def _is_kw_only(a_type):
    if isinstance(a_type, str):
        return _annotation_name(a_type) == 'KW_ONLY'
    return a_type is KW_ONLY


def _typing():
    try:
        import typing
    except ImportError:
        return None
    return typing


def _unhashable_class(cls):
    for c in cls.__mro__:
        if c is list or c is dict or c is set:
            return True
        try:
            d = c.__dict__
        except AttributeError:
            continue
        if '__hash__' in d:
            return d['__hash__'] is None
    return False


def _get_field(cls, a_name, a_type, default_kw_only):
    default = getattr(cls, a_name, MISSING)
    if isinstance(default, Field):
        f = default
    else:
        f = field(default=default)

    f.name = a_name
    f.type = a_type
    f._field_type = _FIELD

    if _is_classvar(a_type):
        f._field_type = _FIELD_CLASSVAR
    elif _is_initvar(a_type):
        f._field_type = _FIELD_INITVAR

    if f._field_type is _FIELD_CLASSVAR or f._field_type is _FIELD_INITVAR:
        if f.default_factory is not MISSING:
            raise TypeError(f'field {f.name} cannot have a '
                            'default factory')

    if f._field_type is _FIELD or f._field_type is _FIELD_INITVAR:
        if f.kw_only is MISSING:
            f.kw_only = default_kw_only
    else:
        if f.kw_only is not MISSING:
            raise TypeError(f'field {f.name} is a ClassVar but specifies '
                            'kw_only')

    if f._field_type is _FIELD and _unhashable_class(type(f.default)):
        raise ValueError(f'mutable default {type(f.default)} for field '
                         f'{f.name} is not allowed: use default_factory')

    return f


def _set_new_attribute(cls, name, value):
    if name in cls.__dict__:
        return True
    setattr(cls, name, value)
    return False


def _hash_set_none(cls, fields):
    return None


def _hash_add(cls, fields):
    flds = [f for f in fields if (f.compare if f.hash is None else f.hash)]
    return _hash_fn(cls, flds)


def _hash_exception(cls, fields):
    raise TypeError(f'Cannot overwrite attribute __hash__ '
                    f'in class {cls.__name__}')


_hash_action = {(False, False, False, False): None,
                (False, False, False, True ): None,
                (False, False, True,  False): None,
                (False, False, True,  True ): None,
                (False, True,  False, False): _hash_set_none,
                (False, True,  False, True ): None,
                (False, True,  True,  False): _hash_add,
                (False, True,  True,  True ): None,
                (True,  False, False, False): _hash_add,
                (True,  False, False, True ): _hash_exception,
                (True,  False, True,  False): _hash_add,
                (True,  False, True,  True ): _hash_exception,
                (True,  True,  False, False): _hash_add,
                (True,  True,  False, True ): _hash_exception,
                (True,  True,  True,  False): _hash_add,
                (True,  True,  True,  True ): _hash_exception,
                }


def _add_method(cls, name, fn, overwrite_error=False):
    already_exists = _set_new_attribute(cls, name, fn)
    if already_exists and overwrite_error:
        error_msg = (f'Cannot overwrite attribute {name} '
                     f'in class {cls.__name__}')
        if overwrite_error is not True:
            error_msg = f'{error_msg} {overwrite_error}'
        raise TypeError(error_msg)


def _process_class(cls, init, repr, eq, order, unsafe_hash, frozen,
                   match_args, kw_only, slots, weakref_slot):
    fields = {}

    setattr(cls, _PARAMS, _DataclassParams(init, repr, eq, order,
                                           unsafe_hash, frozen,
                                           match_args, kw_only,
                                           slots, weakref_slot))

    any_frozen_base = False
    all_frozen_bases = None
    has_dataclass_bases = False
    for b in cls.__mro__[-1:0:-1]:
        base_fields = getattr(b, _FIELDS, None)
        if base_fields is not None:
            has_dataclass_bases = True
            for f in base_fields.values():
                fields[f.name] = f
            if all_frozen_bases is None:
                all_frozen_bases = True
            current_frozen = getattr(b, _PARAMS).frozen
            all_frozen_bases = all_frozen_bases and current_frozen
            any_frozen_base = any_frozen_base or current_frozen

    cls_annotations = cls.__dict__.get('__annotations__', {})

    cls_fields = []
    KW_ONLY_seen = False
    for name, type_ in cls_annotations.items():
        if _is_kw_only(type_):
            if KW_ONLY_seen:
                raise TypeError(f'{name!r} is KW_ONLY, but KW_ONLY '
                                'has already been specified')
            KW_ONLY_seen = True
            kw_only = True
        else:
            cls_fields.append(_get_field(cls, name, type_, kw_only))

    for f in cls_fields:
        fields[f.name] = f
        if isinstance(getattr(cls, f.name, None), Field):
            if f.default is MISSING:
                delattr(cls, f.name)
            else:
                setattr(cls, f.name, f.default)

    for name, value in cls.__dict__.items():
        if isinstance(value, Field) and not name in cls_annotations:
            raise TypeError(f'{name!r} is a field but has no type annotation')

    if has_dataclass_bases:
        if any_frozen_base and not frozen:
            raise TypeError('cannot inherit non-frozen dataclass from a '
                            'frozen one')
        if all_frozen_bases is False and frozen:
            raise TypeError('cannot inherit frozen dataclass from a '
                            'non-frozen one')

    setattr(cls, _FIELDS, fields)

    class_hash = cls.__dict__.get('__hash__', MISSING)
    has_explicit_hash = not (class_hash is MISSING or
                             (class_hash is None and '__eq__' in cls.__dict__))

    if order and not eq:
        raise ValueError('eq must be true if order is true')

    all_init_fields = [f for f in fields.values()
                       if f._field_type is _FIELD or f._field_type is _FIELD_INITVAR]
    (std_init_fields,
     kw_only_init_fields) = _fields_in_init_order(all_init_fields)

    if init:
        has_post_init = hasattr(cls, _POST_INIT_NAME)
        fn = _init_fn(cls, all_init_fields, std_init_fields,
                      kw_only_init_fields, frozen, has_post_init,
                      '__dataclass_self__' if 'self' in fields else 'self')
        _add_method(cls, '__init__', fn)

    _set_new_attribute(cls, '__replace__', _replace)

    field_list = [f for f in fields.values() if f._field_type is _FIELD]

    if repr:
        _add_method(cls, '__repr__', _repr_fn(cls, [f for f in field_list if f.repr]))

    if eq:
        _add_method(cls, '__eq__', _eq_fn(cls, [f for f in field_list if f.compare]))

    if order:
        flds = [f for f in field_list if f.compare]
        for name in ('__lt__', '__le__', '__gt__', '__ge__'):
            _add_method(cls, name, _order_fn(cls, flds, name),
                        'Consider using functools.total_ordering')

    if frozen:
        setter, deleter = _frozen_fns(cls, field_list)
        _add_method(cls, '__setattr__', setter, True)
        _add_method(cls, '__delattr__', deleter, True)

    hash_action = _hash_action[bool(unsafe_hash),
                               bool(eq),
                               bool(frozen),
                               has_explicit_hash]
    if hash_action:
        cls.__hash__ = hash_action(cls, field_list)

    if match_args:
        _set_new_attribute(cls, '__match_args__',
                           tuple(f.name for f in std_init_fields))

    if weakref_slot and not slots:
        raise TypeError('weakref_slot is True but slots is False')

    return cls


def dataclass(cls=None, /, *, init=True, repr=True, eq=True, order=False,
              unsafe_hash=False, frozen=False, match_args=True,
              kw_only=False, slots=False, weakref_slot=False):
    """Add dunder methods based on the fields defined in the class."""

    def wrap(cls):
        return _process_class(cls, init, repr, eq, order, unsafe_hash,
                              frozen, match_args, kw_only, slots,
                              weakref_slot)

    if cls is None:
        return wrap

    return wrap(cls)


def fields(class_or_instance):
    """Return a tuple describing the fields of this dataclass."""
    try:
        fields = getattr(class_or_instance, _FIELDS)
    except AttributeError:
        raise TypeError('must be called with a dataclass type or instance') from None
    return tuple(f for f in fields.values() if f._field_type is _FIELD)


def _is_dataclass_instance(obj):
    return hasattr(type(obj), _FIELDS)


def is_dataclass(obj):
    """Returns True if obj is a dataclass or an instance of a
    dataclass."""
    cls = obj if isinstance(obj, type) else type(obj)
    return hasattr(cls, _FIELDS)


def asdict(obj, *, dict_factory=dict):
    """Return the fields of a dataclass instance as a new dictionary mapping
    field names to field values."""
    if not _is_dataclass_instance(obj):
        raise TypeError("asdict() should be called on dataclass instances")
    return _asdict_inner(obj, dict_factory)


def _asdict_inner(obj, dict_factory):
    obj_type = type(obj)
    if obj_type in _ATOMIC_TYPES:
        return obj
    elif hasattr(obj_type, _FIELDS):
        if dict_factory is dict:
            return {
                f.name: _asdict_inner(getattr(obj, f.name), dict)
                for f in fields(obj)
            }
        else:
            return dict_factory([
                (f.name, _asdict_inner(getattr(obj, f.name), dict_factory))
                for f in fields(obj)
            ])
    elif obj_type is list:
        return [_asdict_inner(v, dict_factory) for v in obj]
    elif obj_type is dict:
        return {
            _asdict_inner(k, dict_factory): _asdict_inner(v, dict_factory)
            for k, v in obj.items()
        }
    elif obj_type is tuple:
        return tuple([_asdict_inner(v, dict_factory) for v in obj])
    elif issubclass(obj_type, tuple):
        if hasattr(obj, '_fields'):
            return obj_type(*[_asdict_inner(v, dict_factory) for v in obj])
        else:
            return obj_type(_asdict_inner(v, dict_factory) for v in obj)
    elif issubclass(obj_type, dict):
        if hasattr(obj_type, 'default_factory'):
            result = obj_type(obj.default_factory)
            for k, v in obj.items():
                result[_asdict_inner(k, dict_factory)] = _asdict_inner(v, dict_factory)
            return result
        return obj_type((_asdict_inner(k, dict_factory),
                         _asdict_inner(v, dict_factory))
                        for k, v in obj.items())
    elif issubclass(obj_type, list):
        return obj_type(_asdict_inner(v, dict_factory) for v in obj)
    else:
        return copy.deepcopy(obj)


def astuple(obj, *, tuple_factory=tuple):
    """Return the fields of a dataclass instance as a new tuple of field
    values."""
    if not _is_dataclass_instance(obj):
        raise TypeError("astuple() should be called on dataclass instances")
    return _astuple_inner(obj, tuple_factory)


def _astuple_inner(obj, tuple_factory):
    if type(obj) in _ATOMIC_TYPES:
        return obj
    elif _is_dataclass_instance(obj):
        return tuple_factory([
            _astuple_inner(getattr(obj, f.name), tuple_factory)
            for f in fields(obj)
        ])
    elif isinstance(obj, tuple) and hasattr(obj, '_fields'):
        return type(obj)(*[_astuple_inner(v, tuple_factory) for v in obj])
    elif isinstance(obj, (list, tuple)):
        return type(obj)(_astuple_inner(v, tuple_factory) for v in obj)
    elif isinstance(obj, dict):
        obj_type = type(obj)
        if hasattr(obj_type, 'default_factory'):
            result = obj_type(getattr(obj, 'default_factory'))
            for k, v in obj.items():
                result[_astuple_inner(k, tuple_factory)] = _astuple_inner(v, tuple_factory)
            return result
        return obj_type((_astuple_inner(k, tuple_factory), _astuple_inner(v, tuple_factory))
                        for k, v in obj.items())
    else:
        return copy.deepcopy(obj)


def make_dataclass(cls_name, fields, *, bases=(), namespace=None, init=True,
                   repr=True, eq=True, order=False, unsafe_hash=False,
                   frozen=False, match_args=True, kw_only=False, slots=False,
                   weakref_slot=False, module=None, decorator=dataclass):
    """Return a new dynamically created dataclass."""
    if namespace is None:
        namespace = {}

    seen = set()
    annotations = {}
    defaults = {}
    for item in fields:
        if isinstance(item, str):
            name = item
            tp = 'typing.Any'
        elif len(item) == 2:
            name, tp, = item
        elif len(item) == 3:
            name, tp, spec = item
            defaults[name] = spec
        else:
            raise TypeError(f'Invalid field: {item!r}')

        if not isinstance(name, str) or not name.isidentifier():
            raise TypeError(f'Field names must be valid identifiers: {name!r}')
        if name in seen:
            raise TypeError(f'Field name duplicated: {name!r}')

        seen.add(name)
        annotations[name] = tp

    ns = dict(namespace)
    ns.update(defaults)
    ns['__annotations__'] = annotations
    if module is not None:
        ns['__module__'] = module
    cls = type(cls_name, tuple(bases), ns)

    return decorator(cls, init=init, repr=repr, eq=eq, order=order,
                     unsafe_hash=unsafe_hash, frozen=frozen,
                     match_args=match_args, kw_only=kw_only, slots=slots,
                     weakref_slot=weakref_slot)


def replace(obj, /, **changes):
    """Return a new object replacing specified fields with new values."""
    if not _is_dataclass_instance(obj):
        raise TypeError("replace() should be called on dataclass instances")
    return _replace(obj, **changes)


def _replace(self, /, **changes):
    for f in getattr(self, _FIELDS).values():
        if f._field_type is _FIELD_CLASSVAR:
            continue

        if not f.init:
            if f.name in changes:
                raise TypeError(f'field {f.name} is declared with '
                                f'init=False, it cannot be specified with '
                                f'replace()')
            continue

        if f.name not in changes:
            if f._field_type is _FIELD_INITVAR and f.default is MISSING:
                raise TypeError(f"InitVar {f.name!r} "
                                f'must be specified with replace()')
            changes[f.name] = getattr(self, f.name)

    return type(self)(**changes)
