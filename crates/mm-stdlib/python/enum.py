# enum, adapted from CPython 3.14's Lib/enum.py (PSF license, see
# THIRD_PARTY.md). micro-monty has no `__prepare__`: `EnumType.__new__`
# finds the members in the class namespace (in definition order) and
# resolves `auto()` there. No pickling support, `_ignore_` or `_order_`
# checks; boundaries other than STRICT/CONFORM/EJECT/KEEP as in CPython.

__all__ = [
    'EnumType', 'EnumMeta', 'Enum', 'IntEnum', 'StrEnum', 'Flag', 'IntFlag',
    'ReprEnum', 'auto', 'unique', 'property', 'verify', 'member', 'nonmember',
    'FlagBoundary', 'STRICT', 'CONFORM', 'EJECT', 'KEEP', 'global_enum',
    'EnumCheck', 'CONTINUOUS', 'NAMED_FLAGS', 'UNIQUE', 'pickle_by_global_name',
    'pickle_by_enum_name', 'show_flag_values',
]

_builtin_property = property
_bltin_bin = bin
_BUILTIN_MIX = (int, str, float, tuple, bytes, complex)


class nonmember:
    """Protects item from becoming an Enum member during class creation."""

    def __init__(self, value):
        self.value = value


class member:
    """Forces item to become an Enum member during class creation."""

    def __init__(self, value):
        self.value = value


_auto_null = object()


class auto:
    """Instances are replaced with an appropriate value in Enum class suites."""

    def __init__(self, value=_auto_null):
        self.value = value
        self._op = None

    def __repr__(self):
        return 'auto(%r)' % (self.value,)

    # CPython resolves `auto()` as the class body assigns it, so later
    # members can combine earlier ones (`WHITE = RED | BLUE`); here the
    # combination is recorded and computed once the operands are known.
    def _combine(self, op, other, swap=False):
        a = auto()
        a._op = (op, other, self) if swap else (op, self, other)
        return a

    def __or__(self, other):
        return self._combine(lambda x, y: x | y, other)

    def __and__(self, other):
        return self._combine(lambda x, y: x & y, other)

    def __xor__(self, other):
        return self._combine(lambda x, y: x ^ y, other)

    def __ror__(self, other):
        return self._combine(lambda x, y: x | y, other, True)

    def __rand__(self, other):
        return self._combine(lambda x, y: x & y, other, True)

    def __rxor__(self, other):
        return self._combine(lambda x, y: x ^ y, other, True)


def _resolve(v):
    if isinstance(v, auto):
        if v._op is not None:
            return v._op[0](_resolve(v._op[1]), _resolve(v._op[2]))
        return v.value
    return v


property = _builtin_property


def _is_dunder(name):
    return len(name) > 4 and name[:2] == name[-2:] == '__' and name[2] != '_' and name[-3] != '_'


def _is_sunder(name):
    return len(name) > 2 and name[0] == name[-1] == '_' and name[1] != '_' and name[-2] != '_'


def _is_descriptor(obj):
    t = type(obj)
    return (callable(obj) or hasattr(t, '__get__') or hasattr(t, '__set__')
            or hasattr(t, '__delete__') or t is staticmethod or t is classmethod
            or t is _builtin_property)


def _is_single_bit(num):
    if num == 0:
        return False
    num &= num - 1
    return num == 0


def _iter_bits_lsb(num):
    # num must be a positive integer
    original = num
    if isinstance(num, Enum):
        num = num.value
    if num < 0:
        raise ValueError('%r is not a positive integer' % original)
    while num:
        b = num & (~num + 1)
        yield b
        num ^= b


def _high_bit(value):
    return value.bit_length() - 1


def show_flag_values(value):
    return list(_iter_bits_lsb(value))


def pickle_by_global_name(self, proto):
    return self.name


def pickle_by_enum_name(self, proto):
    return getattr, (self.__class__, self._name_)


class FlagBoundary:
    pass


STRICT = 'strict'
CONFORM = 'conform'
EJECT = 'eject'
KEEP = 'keep'
UNIQUE = 'no duplicate values'
CONTINUOUS = 'no skipped integer values'
NAMED_FLAGS = 'multi-flag aliases may not contain unnamed flags'
EnumCheck = FlagBoundary


_not_given = object()


class EnumType(type):
    """Metaclass for Enum"""

    def __new__(metacls, cls, bases, classdict, *, boundary=None, _simple=False, **kwds):
        names = []
        ns = {}
        for key, value in classdict.items():
            if type(value) is nonmember:
                ns[key] = value.value
            elif type(value) is member:
                names.append((key, value.value))
            elif (_is_dunder(key) or _is_sunder(key) or _is_descriptor(value)
                  or (key.startswith('_') and '__' in key)
                  or isinstance(value, type)):
                ns[key] = value
            else:
                names.append((key, value))
        member_type, first_enum = metacls._get_mixins_(cls, bases)
        ns['_member_names_'] = []
        ns['_member_map_'] = {}
        ns['_value2member_map_'] = {}
        ns['_unhashable_values_'] = []
        ns['_member_type_'] = member_type
        ns['_value_repr_'] = None
        if boundary is not None:
            ns['_boundary_'] = boundary
        enum_class = super().__new__(metacls, cls, bases, ns, **kwds)
        if '_boundary_' not in ns:
            enum_class._boundary_ = getattr(first_enum, '_boundary_', None)
        gnv = getattr(enum_class, '_generate_next_value_', None)
        last_values = []
        new = classdict.get('__new__')
        if new is None:
            new = getattr(first_enum, '_new_member_', object.__new__)
            if new is object.__new__ and member_type is not object:
                new = member_type.__new__
        enum_class._new_member_ = new
        enum_class._use_args_ = member_type is not object
        progress = '_%s__in_progress' % cls
        setattr(enum_class, progress, True)
        for name, value in names:
            if isinstance(value, auto) and value._op is not None:
                value = _resolve(value)
            elif isinstance(value, auto):
                if value.value is _auto_null:
                    value.value = gnv(name, 1, len(enum_class._member_names_), last_values[:])
                value = value.value
            elif isinstance(value, tuple) and value and any(isinstance(v, auto) for v in value):
                value = tuple(gnv(name, 1, len(enum_class._member_names_), last_values[:])
                              if isinstance(v, auto) else v for v in value)
            last_values.append(value)
            enum_class._add_member_(name, value)
        delattr(enum_class, progress)
        own = set(classdict)
        if ReprEnum is not None and ReprEnum in bases:
            # ReprEnum leaves str() and format() to the mixed-in type.
            if member_type is object:
                raise TypeError('ReprEnum subclasses must be mixed with a data type (i.e.'
                                ' int, str, float, etc.)')
            if '__format__' not in own:
                enum_class.__format__ = member_type.__format__
                own.add('__format__')
            if '__str__' not in own:
                method = member_type.__str__
                if method is object.__str__:
                    method = member_type.__repr__
                enum_class.__str__ = method
                own.add('__str__')
        if member_type is not object:
            # The mixin's own repr/str/format come first in the MRO; CPython
            # puts the enum's back on the class.
            for name in ('__repr__', '__str__', '__format__'):
                if name not in own:
                    for b in enum_class.__mro__[1:]:
                        if isinstance(b, EnumType) and name in b.__dict__:
                            setattr(enum_class, name, b.__dict__[name])
                            break
        if Flag is not None and issubclass(enum_class, Flag):
            single = mask = 0
            for m in enum_class._member_map_.values():
                if _is_single_bit(m._value_):
                    single |= m._value_
                mask |= m._value_
            enum_class._flag_mask_ = mask
            enum_class._singles_mask_ = single
            enum_class._all_bits_ = 2 ** mask.bit_length() - 1
            enum_class._inverted_ = None
            values = [m._value_ for m in enum_class]
            if values != sorted(values):
                enum_class._iter_member_ = enum_class._iter_member_by_def_
        if Enum is not None:
            # As CPython: every enum's `__new__` is the value lookup (a
            # member `__new__` is `_new_member_`).
            enum_class.__new__ = Enum.__dict__['__new__']
        return enum_class

    def _add_member_(cls, name, value):
        if name in cls._member_map_:
            raise TypeError('%r already defined as %r' % (name, cls._member_map_[name]))
        args = value if isinstance(value, tuple) else (value,)
        if cls._member_type_ is tuple:
            args = (args,)
        new = cls._new_member_
        if new is object.__new__ or not cls._use_args_:
            enum_member = new(cls) if new is object.__new__ else new(cls, *args)
        else:
            enum_member = new(cls, *args)
        if not hasattr(enum_member, '_value_'):
            if cls._member_type_ is object:
                enum_member._value_ = value
            else:
                enum_member._value_ = cls._member_type_(*args)
        value = enum_member._value_
        enum_member._name_ = name
        enum_member.__objclass__ = cls
        enum_member._sort_order_ = len(cls._member_names_)
        for b in cls.__mro__:
            if isinstance(b, EnumType) and '__init__' in b.__dict__:
                enum_member.__init__(*args)
                break
        canonical = None
        try:
            canonical = cls._value2member_map_.get(value)
        except TypeError:
            for m in cls._member_map_.values():
                if m._value_ == value:
                    canonical = m
                    break
        if canonical is None:
            for m in cls._unhashable_values_:
                if m._value_ == value:
                    canonical = m
                    break
        if canonical is not None:
            enum_member = canonical
        elif Flag is None or not issubclass(cls, Flag) or _is_single_bit(value):
            cls._member_names_.append(name)
        type.__setattr__(cls, name, enum_member)
        cls._member_map_[name] = enum_member
        try:
            if value not in cls._value2member_map_:
                cls._value2member_map_[value] = enum_member
        except TypeError:
            cls._unhashable_values_.append(enum_member)

    def __call__(cls, value, names=_not_given, *values, module=None, qualname=None,
                 type=None, start=1, boundary=None):
        if cls._member_map_:
            if names is not _not_given:
                value = (value, names) + values
            return Enum.__new__(cls, value)
        if names is _not_given and type is None:
            raise TypeError('%s has no members; specify `names=()` if you meant to create'
                            ' a new, empty, enum' % (cls,))
        return cls._create_(value, None if names is _not_given else names, module=module,
                            qualname=qualname, type=type, start=start, boundary=boundary)

    def __contains__(cls, value):
        if isinstance(value, cls):
            return True
        try:
            return value in cls._value2member_map_
        except TypeError:
            return any(m._value_ == value for m in cls._unhashable_values_)

    def __setattr__(cls, name, value):
        if name in cls.__dict__.get('_member_map_', ()):
            raise AttributeError('cannot reassign member %r' % (name,))
        super().__setattr__(name, value)

    def __delattr__(cls, attr):
        if attr in cls._member_map_:
            raise AttributeError('%r cannot delete member %r.' % (cls.__name__, attr))
        super().__delattr__(attr)

    def __dir__(cls):
        interesting = set([
                '__class__', '__contains__', '__doc__', '__getitem__',
                '__iter__', '__len__', '__members__', '__module__',
                '__name__', '__qualname__',
                ]
                + cls._member_names_
                )
        if cls._new_member_ is not object.__new__:
            interesting.add('__new__')
        if cls.__init_subclass__ is not object.__init_subclass__:
            interesting.add('__init_subclass__')
        if cls._member_type_ is object:
            return sorted(interesting)
        else:
            # return whatever mixed-in data type has
            return sorted(set(dir(cls._member_type_)) | interesting)

    def __getitem__(cls, name):
        return cls._member_map_[name]

    def __iter__(cls):
        return (cls._member_map_[name] for name in cls._member_names_)

    def __len__(cls):
        return len(cls._member_names_)

    @_builtin_property
    def __members__(cls):
        return dict(cls._member_map_)

    def __repr__(cls):
        if Flag is not None and issubclass(cls, Flag):
            return "<flag %r>" % cls.__name__
        return "<enum %r>" % cls.__name__

    def __reversed__(cls):
        return (cls._member_map_[name] for name in reversed(cls._member_names_))

    def __bool__(cls):
        return True

    def _create_(cls, class_name, names, *, module=None, qualname=None, type=None, start=1,
                 boundary=None):
        metacls = cls.__class__
        bases = (cls,) if type is None else (type, cls)
        if isinstance(names, str):
            names = names.replace(',', ' ').split()
        classdict = {}
        if isinstance(names, (tuple, list)) and names and isinstance(names[0], str):
            original_names, names = names, []
            last_values = []
            for count, name in enumerate(original_names):
                value = cls._generate_next_value_(name, start, count, last_values[:])
                last_values.append(value)
                names.append((name, value))
        if names is None:
            names = ()
        if not isinstance(names, (tuple, list)):
            names = list(names.items())
        for item in names:
            if isinstance(item, str):
                member_name, member_value = item, names[item]
            else:
                member_name, member_value = item
            classdict[member_name] = member_value
        classdict['__module__'] = module if module is not None else '__main__'
        if qualname is not None:
            classdict['__qualname__'] = qualname
        return metacls(class_name, bases, classdict, boundary=boundary)

    @classmethod
    def _get_mixins_(mcls, class_name, bases):
        if not bases:
            return object, Enum
        for chain in bases:
            for base in chain.__mro__:
                if isinstance(base, EnumType) and base._member_names_:
                    raise TypeError('<enum %r> cannot extend %r' % (class_name, base))
        member_type = object
        for chain in bases:
            for base in chain.__mro__:
                if base is object:
                    continue
                if isinstance(base, EnumType):
                    if base._member_type_ is not object:
                        member_type = base._member_type_
                        break
                elif base in _BUILTIN_MIX or '__new__' in getattr(base, '__dict__', {}):
                    member_type = base
                    break
                else:
                    continue
            if member_type is not object:
                break
        first_enum = bases[-1]
        if not isinstance(first_enum, EnumType):
            raise TypeError("new enumerations should be created as "
                            "`EnumName([mixin_type, ...] [data_type,] enum_type)`")
        return member_type, first_enum


EnumMeta = EnumType
Enum = Flag = ReprEnum = None


class Enum(metaclass=EnumType):
    """Create a collection of name/value pairs."""

    def __new__(cls, value):
        if type(value) is cls:
            return value
        try:
            return cls._value2member_map_[value]
        except (KeyError, TypeError):
            pass
        for m in cls._unhashable_values_:
            if m._value_ == value:
                return m
        # Members being created: a member `__new__` called `super().__new__`.
        if not cls._member_map_ and getattr(cls, '_%s__in_progress' % cls.__name__, False):
            raise TypeError('do not use `super().__new__; call the appropriate __new__ directly') from None
        exc = None
        try:
            result = cls._missing_(value)
        except Exception as e:
            exc = e
            result = None
        if isinstance(result, cls):
            return result
        if Flag is not None and issubclass(cls, Flag) and cls._boundary_ is EJECT and isinstance(result, int):
            return result
        ve = ValueError('%r is not a valid %s' % (value, cls.__qualname__))
        if result is None and exc is None:
            raise ve
        if exc is None:
            exc = TypeError('error in %s._missing_: returned %r instead of None or a valid member'
                            % (cls.__name__, result))
        raise exc

    def _generate_next_value_(name, start, count, last_values):
        if not last_values:
            return start
        last = last_values[-1]
        try:
            return last + 1
        except TypeError:
            raise TypeError('unable to sort non-numeric values') from None

    @classmethod
    def _missing_(cls, value):
        return None

    def __repr__(self):
        v_repr = self.__class__._value_repr_ or repr
        return '<%s.%s: %s>' % (self.__class__.__name__, self._name_, v_repr(self._value_))

    def __str__(self):
        return '%s.%s' % (self.__class__.__name__, self._name_)

    def __dir__(self):
        interesting = set()
        if self.__class__._member_type_ is not object:
            interesting = set(object.__dir__(self))
        for name in getattr(self, '__dict__', []):
            if name[0] != '_' and name not in self._member_map_:
                interesting.add(name)
        for cls in self.__class__.mro():
            for name, obj in cls.__dict__.items():
                if name[0] == '_':
                    continue
                if isinstance(obj, property):
                    # that's an enum.property
                    if obj.fget is not None or name not in self._member_map_:
                        interesting.add(name)
                    else:
                        # in case it was added by `dir(self)`
                        interesting.discard(name)
                elif name not in self._member_map_:
                    interesting.add(name)
        names = sorted(
                set(['__class__', '__doc__', '__eq__', '__hash__', '__module__'])
                | interesting
                )
        return names

    def __format__(self, format_spec):
        return format(str(self), format_spec)

    def __hash__(self):
        return hash(self._name_)

    def __reduce_ex__(self, proto):
        return self.__class__, (self._value_,)

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        return self

    @_builtin_property
    def name(self):
        """The name of the Enum member."""
        return self._name_

    @_builtin_property
    def value(self):
        """The value of the Enum member."""
        return self._value_


Enum._new_member_ = object.__new__


class ReprEnum(Enum):
    """Only changes the repr(), leaving str() and format() to the mixed-in type."""


class IntEnum(int, ReprEnum):
    """Enum where members are also (and must be) ints"""

    def __str__(self):
        return repr(self._value_)

    def __format__(self, format_spec):
        # As `int.__format__`/`str.__format__`: an empty spec is `str(self)`,
        # so a subclass's own `__str__` shows.
        if not format_spec:
            return str(self)
        return format(self._value_, format_spec)


class StrEnum(str, ReprEnum):
    """Enum where members are also (and must be) strings"""

    def __new__(cls, *values):
        if len(values) > 3:
            raise TypeError('too many arguments for str(): %r' % (values,))
        if len(values) == 1 and not isinstance(values[0], str):
            raise TypeError('%r is not a string' % (values[0],))
        value = str(*values)
        member = str.__new__(cls, value)
        member._value_ = value
        return member

    def __str__(self):
        return self._value_

    def __format__(self, format_spec):
        # As `int.__format__`/`str.__format__`: an empty spec is `str(self)`,
        # so a subclass's own `__str__` shows.
        if not format_spec:
            return str(self)
        return format(self._value_, format_spec)

    def _generate_next_value_(name, start, count, last_values):
        return name.lower()




class Flag(Enum, boundary=STRICT):
    """Support for flags"""

    _numeric_repr_ = repr

    def _generate_next_value_(name, start, count, last_values):
        if not count:
            return start if start is not None else 1
        last_value = max(last_values)
        try:
            high_bit = _high_bit(last_value)
        except Exception:
            raise TypeError('invalid flag value %r' % last_value) from None
        return 2 ** (high_bit + 1)

    @classmethod
    def _iter_member_by_value_(cls, value):
        for val in _iter_bits_lsb(value & cls._flag_mask_):
            yield cls._value2member_map_.get(val)

    _iter_member_ = _iter_member_by_value_

    @classmethod
    def _iter_member_by_def_(cls, value):
        yield from sorted(cls._iter_member_by_value_(value), key=lambda m: m._sort_order_)

    @classmethod
    def _missing_(cls, value):
        if not isinstance(value, int):
            raise ValueError('%r is not a valid %s' % (value, cls.__qualname__))
        flag_mask = cls._flag_mask_
        singles_mask = cls._singles_mask_
        all_bits = cls._all_bits_
        neg_value = None
        if (not ~all_bits <= value <= all_bits or value & (all_bits ^ flag_mask)):
            if cls._boundary_ is STRICT:
                max_bits = max(value.bit_length(), flag_mask.bit_length())
                raise ValueError('%r invalid value %r\n    given %s\n  allowed %s' % (
                    cls, value, bin(value, max_bits), bin(flag_mask, max_bits)))
            elif cls._boundary_ is CONFORM:
                value = value & flag_mask
            elif cls._boundary_ is EJECT:
                return value
            elif cls._boundary_ is KEEP:
                if value < 0:
                    value = max(all_bits + 1, 2 ** value.bit_length()) + value
            else:
                raise ValueError('%r unknown flag boundary %r' % (cls, cls._boundary_))
        if value < 0:
            neg_value = value
            value = all_bits + 1 + value
        unknown = value & ~flag_mask
        aliases = value & ~singles_mask
        member_value = value & singles_mask
        if unknown and cls._boundary_ is not KEEP:
            raise ValueError('%s(%r) -->  unknown values %r [%s]' % (
                cls.__name__, value, unknown, bin(unknown)))
        if cls._member_type_ is object:
            pseudo_member = object.__new__(cls)
        else:
            pseudo_member = cls._member_type_.__new__(cls, value)
        if not hasattr(pseudo_member, '_value_'):
            pseudo_member._value_ = value
        if member_value or aliases:
            members = []
            combined_value = 0
            for m in cls._iter_member_(member_value):
                members.append(m)
                combined_value |= m._value_
            if aliases:
                value = member_value | aliases
                for n, pv in cls._member_map_.items():
                    if pv not in members and pv._value_ and pv._value_ & value == pv._value_:
                        members.append(pv)
                        combined_value |= pv._value_
            unknown = value ^ combined_value
            pseudo_member._name_ = '|'.join([m._name_ for m in members])
            if not combined_value:
                pseudo_member._name_ = None
            elif unknown and cls._boundary_ is STRICT:
                raise ValueError('%r: no members with value %r' % (cls, unknown))
            elif unknown:
                pseudo_member._name_ += '|%s' % cls._numeric_repr_(unknown)
        else:
            pseudo_member._name_ = None
        pseudo_member = cls._value2member_map_.setdefault(value, pseudo_member)
        if neg_value is not None:
            cls._value2member_map_[neg_value] = pseudo_member
        return pseudo_member

    def __contains__(self, other):
        if not isinstance(other, self.__class__):
            raise TypeError("unsupported operand type(s) for 'in': %r and %r" % (
                type(other).__qualname__, self.__class__.__qualname__))
        return other._value_ & self._value_ == other._value_

    def __iter__(self):
        yield from self._iter_member_(self._value_)

    def __len__(self):
        return self._value_.bit_count()

    def __repr__(self):
        cls_name = self.__class__.__name__
        v_repr = self.__class__._value_repr_ or repr
        if self._name_ is None:
            return '<%s: %s>' % (cls_name, v_repr(self._value_))
        return '<%s.%s: %s>' % (cls_name, self._name_, v_repr(self._value_))

    def __str__(self):
        cls_name = self.__class__.__name__
        if self._name_ is None:
            return '%s(%r)' % (cls_name, self._value_)
        return '%s.%s' % (cls_name, self._name_)

    def __bool__(self):
        return bool(self._value_)

    def _get_value(self, flag):
        if isinstance(flag, self.__class__):
            return flag._value_
        elif self._member_type_ is not object and isinstance(flag, self._member_type_):
            return flag
        return NotImplemented

    def __or__(self, other):
        other_value = self._get_value(other)
        if other_value is NotImplemented:
            return NotImplemented
        return self.__class__(self._value_ | other_value)

    def __and__(self, other):
        other_value = self._get_value(other)
        if other_value is NotImplemented:
            return NotImplemented
        return self.__class__(self._value_ & other_value)

    def __xor__(self, other):
        other_value = self._get_value(other)
        if other_value is NotImplemented:
            return NotImplemented
        return self.__class__(self._value_ ^ other_value)

    def __invert__(self):
        if self._get_value(self) is None:
            raise TypeError("'%s' cannot be inverted" % self)
        if self._inverted_ is None:
            if self._boundary_ in (EJECT, KEEP):
                self._inverted_ = self.__class__(~self._value_)
            else:
                self._inverted_ = self.__class__(self._singles_mask_ & ~self._value_)
        return self._inverted_

    __rand__ = __and__
    __ror__ = __or__
    __rxor__ = __xor__


class IntFlag(int, ReprEnum, Flag, boundary=KEEP):
    """Support for integer-based Flags"""

    def __str__(self):
        return repr(self._value_)

    def __format__(self, format_spec):
        # As `int.__format__`/`str.__format__`: an empty spec is `str(self)`,
        # so a subclass's own `__str__` shows.
        if not format_spec:
            return str(self)
        return format(self._value_, format_spec)

    def __or__(self, other):
        if isinstance(other, int) and not isinstance(other, Flag):
            return self.__class__(self._value_ | other)
        return Flag.__or__(self, other)

    def __and__(self, other):
        if isinstance(other, int) and not isinstance(other, Flag):
            return self.__class__(self._value_ & other)
        return Flag.__and__(self, other)

    def __xor__(self, other):
        if isinstance(other, int) and not isinstance(other, Flag):
            return self.__class__(self._value_ ^ other)
        return Flag.__xor__(self, other)

    __ror__ = __or__
    __rand__ = __and__
    __rxor__ = __xor__
    __invert__ = Flag.__invert__


def bin(num, max_bits=None):
    ceiling = 2 ** (num).bit_length()
    if num >= 0:
        s = _bltin_bin(num + ceiling).replace('1', '0', 1)
    else:
        s = _bltin_bin(~num ^ (ceiling - 1) + ceiling)
    sign = s[:3]
    digits = s[3:]
    if max_bits is not None:
        if len(digits) < max_bits:
            digits = (sign[-1] * max_bits + digits)[-max_bits:]
    return '%s %s' % (sign, digits)


def unique(enumeration):
    """Class decorator for enumerations ensuring unique member values."""
    duplicates = []
    for name, m in enumeration.__members__.items():
        if name != m.name:
            duplicates.append((name, m.name))
    if duplicates:
        alias_details = ', '.join(['%s -> %s' % (alias, name) for (alias, name) in duplicates])
        raise ValueError('duplicate values found in %r: %s' % (enumeration, alias_details))
    return enumeration


def verify(*checks):
    def decorator(enumeration):
        return enumeration
    return decorator


def global_enum_repr(self):
    """
    use module.enum_name instead of class.enum_name

    the module is the last module in case of a multi-module name
    """
    module = self.__class__.__module__.split('.')[-1]
    return '%s.%s' % (module, self._name_)

def global_flag_repr(self):
    """
    use module.flag_name instead of class.flag_name

    the module is the last module in case of a multi-module name
    """
    module = self.__class__.__module__.split('.')[-1]
    cls_name = self.__class__.__name__
    if self._name_ is None:
        return "%s.%s(%r)" % (module, cls_name, self._value_)
    if _is_single_bit(self._value_):
        return '%s.%s' % (module, self._name_)
    if self._boundary_ is not KEEP:
        return '|'.join(['%s.%s' % (module, name) for name in self.name.split('|')])
    else:
        name = []
        for n in self._name_.split('|'):
            if n[0].isdigit():
                name.append(n)
            else:
                name.append('%s.%s' % (module, n))
        return '|'.join(name)

def global_str(self):
    """
    use enum_name instead of class.enum_name
    """
    if self._name_ is None:
        cls_name = self.__class__.__name__
        return "%s(%r)" % (cls_name, self._value_)
    else:
        return self._name_

def global_enum(cls, update_str=False):
    # CPython's, except that the members are not exported into the module's
    # globals: there is no sys.modules, so the module does that itself.
    if issubclass(cls, Flag):
        cls.__repr__ = global_flag_repr
    else:
        cls.__repr__ = global_enum_repr
    if not issubclass(cls, ReprEnum) or update_str:
        cls.__str__ = global_str
    return cls
