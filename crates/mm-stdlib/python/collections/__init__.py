# collections: adapted from CPython 3.14's Lib/collections/__init__.py
# (PSF license, see THIRD_PARTY.md). namedtuple builds its methods without
# exec; deque is written here (CPython's is C).

from _mm import backing as _backing, signature as _signature

__all__ = ['ChainMap', 'Counter', 'OrderedDict', 'UserDict', 'UserList',
           'UserString', 'defaultdict', 'deque', 'namedtuple']

_KEYWORDS = {'False', 'None', 'True', 'and', 'as', 'assert', 'async', 'await',
             'break', 'class', 'continue', 'def', 'del', 'elif', 'else',
             'except', 'finally', 'for', 'from', 'global', 'if', 'import', 'in',
             'is', 'lambda', 'nonlocal', 'not', 'or', 'pass', 'raise', 'return',
             'try', 'while', 'with', 'yield'}

_tuple_new = tuple.__new__


# `deque[int]` is a `types.GenericAlias` (the builtin containers' class
# subscription, with its arity check).
_class_getitem = list.__class_getitem__.__func__


_repr_running = set()


def _repr_enter(obj):
    # Py_ReprEnter: is `obj` already being repr'd (a self-reference)?
    k = id(obj)
    if k in _repr_running:
        return True
    _repr_running.add(k)
    return False


def _repr_leave(obj):
    _repr_running.discard(id(obj))


################################################################################
### namedtuple
################################################################################

def _field_getter(index):
    def get(self):
        return self[index]
    return property(get, doc=f'Alias for field number {index}')


def namedtuple(typename, field_names, *, rename=False, defaults=None, module=None):
    if isinstance(field_names, str):
        field_names = field_names.replace(',', ' ').split()
    field_names = list(map(str, field_names))
    typename = str(typename)
    if rename:
        seen = set()
        for index, name in enumerate(field_names):
            if (not name.isidentifier() or name in _KEYWORDS
                    or name.startswith('_') or name in seen):
                field_names[index] = f'_{index}'
            seen.add(name)
    for name in [typename] + field_names:
        if type(name) is not str:
            raise TypeError('Type names and field names must be strings')
        if not name.isidentifier():
            raise ValueError('Type names and field names must be valid '
                             f'identifiers: {name!r}')
        if name in _KEYWORDS:
            raise ValueError('Type names and field names cannot be a '
                             f'keyword: {name!r}')
    seen = set()
    for name in field_names:
        if name.startswith('_') and not rename:
            raise ValueError('Field names cannot start with an underscore: '
                             f'{name!r}')
        if name in seen:
            raise ValueError(f'Encountered duplicate field name: {name!r}')
        seen.add(name)

    field_defaults = {}
    if defaults is not None:
        defaults = tuple(defaults)
        if len(defaults) > len(field_names):
            raise TypeError('Got more default values than field names')
        field_defaults = dict(reversed(list(zip(reversed(field_names),
                                                reversed(defaults)))))

    field_names = tuple(field_names)
    num_fields = len(field_names)
    arg_list = ', '.join(field_names)
    if num_fields == 1:
        arg_list += ','

    def _new_impl(_cls, *args):
        return _tuple_new(_cls, args)

    __new__ = _signature('__new__', f'{typename}.__new__', ('_cls',) + field_names,
                            (), defaults, None, _new_impl)

    @classmethod
    def _make(cls, iterable):
        result = _tuple_new(cls, iterable)
        if len(result) != num_fields:
            raise TypeError(f'Expected {num_fields} arguments, got {len(result)}')
        return result

    def _replace(self, /, **kwds):
        result = self._make(map(kwds.pop, field_names, self))
        if kwds:
            raise TypeError(f'Got unexpected field names: {list(kwds)!r}')
        return result

    def __repr__(self):
        parts = [f'{name}={value!r}' for name, value in zip(field_names, self)]
        return self.__class__.__name__ + '(' + ', '.join(parts) + ')'

    def _asdict(self):
        return dict(zip(self._fields, self))

    def __getnewargs__(self):
        return tuple(self)

    class_namespace = {
        '__doc__': f'{typename}({arg_list})',
        '__qualname__': typename,
        '_fields': field_names,
        '_field_defaults': field_defaults,
        '__new__': __new__,
        '_make': _make,
        '__replace__': _replace,
        '_replace': _replace,
        '__repr__': __repr__,
        '_asdict': _asdict,
        '__getnewargs__': __getnewargs__,
        '__match_args__': field_names,
    }
    for index, name in enumerate(field_names):
        class_namespace[name] = _field_getter(index)

    result = type(typename, (tuple,), class_namespace)
    # CPython: the caller's module name, else '__main__' (user code in a
    # bare namespace has none).
    result.__module__ = '__main__' if module is None else module
    return result


################################################################################
### deque
################################################################################

def _index_arg(x):
    # PyNumber_AsSsize_t: an int or __index__, clamped messages as CPython.
    if isinstance(x, int):
        return int(x)
    if hasattr(type(x), '__index__'):
        return x.__index__()
    raise TypeError(f"'{type(x).__name__}' object cannot be interpreted as an integer")


def _ssize(x):
    n = _index_arg(x)
    if n > 9223372036854775807 or n < -9223372036854775808:
        raise OverflowError('Python int too large to convert to C ssize_t')
    return n


def _seq_index(x):
    if isinstance(x, int):
        n = int(x)
    elif hasattr(type(x), '__index__'):
        n = x.__index__()
    else:
        raise TypeError(f"sequence index must be integer, not '{type(x).__name__}'")
    if n > 9223372036854775807 or n < -9223372036854775808:
        raise IndexError("cannot fit 'int' into an index-sized integer")
    return n


def _slice_index(x):
    if isinstance(x, int):
        return int(x)
    if x is not None and hasattr(type(x), '__index__'):
        return x.__index__()
    raise TypeError('slice indices must be integers or have an __index__ method')


def _slice_bound(i, n):
    if i < 0:
        i += n
        if i < 0:
            i = 0
    elif i > n:
        i = n
    return i


class deque:
    __mm_tpname__ = 'collections.deque'
    __abc_tpflags__ = 1 << 5  # a sequence to `match`

    def __init__(self, *args, **kwargs):
        if len(args) > 2:
            raise TypeError(f'deque() takes at most 2 arguments ({len(args)} given)')
        iterable = _MISSING
        maxlen = None
        if args:
            iterable = args[0]
        if len(args) > 1:
            maxlen = args[1]
        for k in kwargs:
            if k == 'iterable':
                iterable = kwargs[k]
            elif k == 'maxlen':
                maxlen = kwargs[k]
            else:
                raise TypeError(f"deque() got an unexpected keyword argument '{k}'")
        if maxlen is not None:
            if not isinstance(maxlen, int):
                raise TypeError('an integer is required')
            maxlen = _ssize(maxlen)
            if maxlen < 0:
                raise ValueError('maxlen must be non-negative')
        self._items = []
        self._head = 0
        self._maxlen = maxlen
        self._state = 0
        if iterable is not _MISSING:
            self.extend(iterable)

    @property
    def maxlen(self):
        return self._maxlen

    @maxlen.setter
    def maxlen(self, value):
        raise AttributeError("attribute 'maxlen' of 'collections.deque' objects is not writable")

    __class_getitem__ = classmethod(_class_getitem)

    def __len__(self):
        return len(self._items) - self._head

    def __bool__(self):
        return len(self._items) > self._head

    def _compact(self):
        h = self._head
        if h > 16 and 2 * h > len(self._items):
            del self._items[:h]
            self._head = 0

    def _list(self):
        return self._items[self._head:]

    def _set_list(self, items):
        self._items = items
        self._head = 0

    def append(self, *args):
        if len(args) != 1:
            raise TypeError(f'deque.append() takes exactly one argument ({len(args)} given)')
        self._state += 1
        self._items.append(args[0])
        if self._maxlen is not None and len(self._items) - self._head > self._maxlen:
            self._items[self._head] = None
            self._head += 1
            self._compact()

    def appendleft(self, *args):
        if len(args) != 1:
            raise TypeError(f'deque.appendleft() takes exactly one argument ({len(args)} given)')
        self._state += 1
        if self._head == 0:
            grow = len(self._items) + 8
            self._items[0:0] = [None] * grow
            self._head = grow
        self._head -= 1
        self._items[self._head] = args[0]
        if self._maxlen is not None and len(self._items) - self._head > self._maxlen:
            self._items.pop()

    def pop(self, *args):
        if args:
            raise TypeError(f'deque.pop() takes no arguments ({len(args)} given)')
        if len(self._items) <= self._head:
            raise IndexError('pop from an empty deque')
        self._state += 1
        return self._items.pop()

    def popleft(self, *args):
        if args:
            raise TypeError(f'deque.popleft() takes no arguments ({len(args)} given)')
        if len(self._items) <= self._head:
            raise IndexError('pop from an empty deque')
        self._state += 1
        x = self._items[self._head]
        self._items[self._head] = None
        self._head += 1
        if self._head == len(self._items):
            self._items = []
            self._head = 0
        else:
            self._compact()
        return x

    def extend(self, *args):
        if len(args) != 1:
            raise TypeError(f'deque.extend() takes exactly one argument ({len(args)} given)')
        it = args[0]
        if it is self:
            it = self._list()
        items = self._items
        maxlen = self._maxlen
        first = True
        for x in it:
            if first:
                self._state += 1
                first = False
            items.append(x)
            if maxlen is not None and len(items) - self._head > maxlen:
                items[self._head] = None
                self._head += 1
        if maxlen is not None:
            self._compact()

    def extendleft(self, *args):
        if len(args) != 1:
            raise TypeError(f'deque.extendleft() takes exactly one argument ({len(args)} given)')
        it = args[0]
        if it is self:
            it = self._list()
        for x in it:
            self.appendleft(x)

    def __iadd__(self, other):
        self.extend(other)
        return self

    def clear(self, *args):
        if args:
            raise TypeError(f'deque.clear() takes no arguments ({len(args)} given)')
        if len(self._items) > self._head:
            self._state += 1
        self._set_list([])

    def copy(self, *args):
        if args:
            raise TypeError(f'deque.copy() takes no arguments ({len(args)} given)')
        d = self.__class__()
        d._maxlen = self._maxlen
        d._set_list(self._list())
        return d

    __copy__ = copy

    def __reduce__(self):
        return type(self), ((), self._maxlen), None, iter(self)

    def count(self, *args):
        if len(args) != 1:
            raise TypeError(f'deque.count() takes exactly one argument ({len(args)} given)')
        x = args[0]
        state = self._state
        n = 0
        for i in range(self._head, len(self._items)):
            item = self._items[i]
            if item is x or item == x:
                n += 1
            if self._state != state:
                raise RuntimeError('deque mutated during iteration')
        return n

    def index(self, *args):
        if len(args) < 1:
            raise TypeError(f'index expected at least 1 argument, got {len(args)}')
        if len(args) > 3:
            raise TypeError(f'index expected at most 3 arguments, got {len(args)}')
        x = args[0]
        # Both bounds are coerced before the length is read.
        bounds = [_slice_index(b) for b in args[1:]]
        n = len(self)
        start = _slice_bound(bounds[0], n) if bounds else 0
        stop = _slice_bound(bounds[1], n) if len(bounds) > 1 else n
        state = self._state
        i = start
        while i < stop:
            item = self._items[self._head + i]
            if item is x or item == x:
                return i
            if self._state != state:
                raise RuntimeError('deque mutated during iteration')
            i += 1
        raise ValueError('deque.index(x): x not in deque')

    def insert(self, *args):
        if len(args) != 2:
            raise TypeError(f'insert expected 2 arguments, got {len(args)}')
        i = _ssize(args[0])
        if self._maxlen is not None and len(self) >= self._maxlen:
            raise IndexError('deque already at its maximum size')
        self._state += 1
        items = self._list()
        items.insert(i, args[1])
        self._set_list(items)

    def remove(self, *args):
        if len(args) != 1:
            raise TypeError(f'deque.remove() takes exactly one argument ({len(args)} given)')
        x = args[0]
        state = self._state
        n = len(self)
        for i in range(n):
            item = self._items[self._head + i]
            found = item is x or item == x
            if self._state != state or len(self) != n:
                raise IndexError('deque mutated during remove().')
            if found:
                self._state += 1
                del self._items[self._head + i]
                return
        raise ValueError('deque.remove(x): x not in deque')

    def reverse(self, *args):
        if args:
            raise TypeError(f'deque.reverse() takes no arguments ({len(args)} given)')
        items = self._list()
        items.reverse()
        self._set_list(items)

    def rotate(self, *args):
        if len(args) > 1:
            raise TypeError(f'rotate expected at most 1 argument, got {len(args)}')
        k = _ssize(args[0]) if args else 1
        n = len(self)
        if n > 1:
            self._state += 1
            k %= n
            if k:
                items = self._list()
                self._set_list(items[n - k:] + items[:n - k])

    def __iter__(self):
        state = self._state
        i = 0
        while True:
            if self._state != state:
                raise RuntimeError('deque mutated during iteration')
            if i >= len(self._items) - self._head:
                return
            yield self._items[self._head + i]
            i += 1

    def __reversed__(self):
        state = self._state
        i = len(self) - 1
        while True:
            if self._state != state:
                raise RuntimeError('deque mutated during iteration')
            if i < 0 or i >= len(self._items) - self._head:
                return
            yield self._items[self._head + i]
            i -= 1

    def __contains__(self, x):
        state = self._state
        for i in range(self._head, len(self._items)):
            item = self._items[i]
            if item is x or item == x:
                return True
            if self._state != state:
                raise RuntimeError('deque mutated during iteration')
        return False

    def _pos(self, i):
        i = _seq_index(i)
        n = len(self)
        if i < 0:
            i += n
        if i < 0 or i >= n:
            raise IndexError('deque index out of range')
        return self._head + i

    def __getitem__(self, i):
        return self._items[self._pos(i)]

    def __setitem__(self, i, v):
        self._items[self._pos(i)] = v

    def __delitem__(self, i):
        p = self._pos(i)
        self._state += 1
        del self._items[p]

    def __repr__(self):
        items = self._list()
        if _repr_enter(self):
            return '[...]'
        try:
            body = repr(items)
        finally:
            _repr_leave(self)
        if self._maxlen is None:
            return f'deque({body})'
        return f'deque({body}, maxlen={self._maxlen})'

    def __eq__(self, other):
        if not isinstance(other, deque):
            return NotImplemented
        return self._list() == other._list()

    def __ne__(self, other):
        if not isinstance(other, deque):
            return NotImplemented
        return self._list() != other._list()

    def __lt__(self, other):
        if not isinstance(other, deque):
            return NotImplemented
        return self._list() < other._list()

    def __le__(self, other):
        if not isinstance(other, deque):
            return NotImplemented
        return self._list() <= other._list()

    def __gt__(self, other):
        if not isinstance(other, deque):
            return NotImplemented
        return self._list() > other._list()

    def __ge__(self, other):
        if not isinstance(other, deque):
            return NotImplemented
        return self._list() >= other._list()

    __hash__ = None

    def __add__(self, other):
        if not isinstance(other, deque):
            raise TypeError(f'can only concatenate deque (not "{type(other).__name__}") to deque')
        d = self.copy()
        d.extend(other._list())
        return d

    def _repeat(self, n):
        n = _index_arg(n)
        if n > 9223372036854775807 or n < -9223372036854775808:
            raise OverflowError("cannot fit 'int' into an index-sized integer")
        items = self._list()
        if n <= 0 or not items:
            return []
        if self._maxlen is not None:
            if self._maxlen == 0:
                return []
            reps = min(n, self._maxlen // len(items) + 2)
            return (items * reps)[-self._maxlen:]
        return items * n

    def __mul__(self, n):
        if not (isinstance(n, int) or hasattr(type(n), '__index__')):
            if (hasattr(type(n), '__rmul__') and type(n).__module__ != 'builtins'
                    and not isinstance(n, (str, bytes, list, tuple, deque))):
                return NotImplemented
            _repeat_error(n)
        d = self.copy()
        d._set_list(self._repeat(n))
        return d

    def __rmul__(self, n):
        if isinstance(n, (str, bytes, list, tuple)):
            # The left sequence's `sq_repeat` runs first: this is the count.
            _repeat_error(self)
        return self.__mul__(n)

    def __imul__(self, n):
        if not (isinstance(n, int) or hasattr(type(n), '__index__')):
            _repeat_error(n)
        items = self._repeat(n)
        self._state += 1
        self._set_list(items)
        return self


def _repeat_error(n):
    # `sq_repeat`'s error, which names the count's type (`tp_name`).
    d = getattr(type(n), '__dict__', None)
    name = type(n).__name__ if d is None else d.get('__mm_tpname__', type(n).__name__)
    raise TypeError(f"can't multiply sequence by non-int of type '{name}'")


################################################################################
### defaultdict
################################################################################

class defaultdict(dict):
    __mm_tpname__ = 'collections.defaultdict'

    def __init__(self, *args, **kwargs):
        factory = None
        if args:
            factory = args[0]
            args = args[1:]
            if factory is not None and not callable(factory):
                raise TypeError('first argument must be callable or None')
        self.default_factory = factory
        dict.update(self, *args, **kwargs)

    def __missing__(self, key):
        factory = self.default_factory
        if factory is None:
            raise KeyError(key)
        value = factory()
        self[key] = value
        return value

    def __repr__(self):
        factory = self.default_factory
        if _repr_enter(self):
            return '...'
        try:
            frepr = repr(factory)
            body = repr(_backing(self))
        finally:
            _repr_leave(self)
        return f'{type(self).__name__}({frepr}, {body})'

    def copy(self):
        return type(self)(self.default_factory, self)

    __copy__ = copy

    def __reduce__(self):
        args = () if self.default_factory is None else (self.default_factory,)
        return type(self), args, None, None, iter(self.items())

    @classmethod
    def fromkeys(cls, *args):
        if len(args) < 1:
            raise TypeError(f'fromkeys expected at least 1 argument, got {len(args)}')
        if len(args) > 2:
            raise TypeError(f'fromkeys expected at most 2 arguments, got {len(args)}')
        value = args[1] if len(args) > 1 else None
        d = cls()
        for k in args[0]:
            d[k] = value
        return d

    def __or__(self, other):
        if not isinstance(other, dict):
            return NotImplemented
        new = self.copy()
        new.update(other)
        return new

    def __ror__(self, other):
        if not isinstance(other, dict):
            return NotImplemented
        new = type(self)(self.default_factory, other)
        new.update(self)
        return new

    def __ior__(self, other):
        self.update(other)
        return self

    __class_getitem__ = classmethod(_class_getitem)


################################################################################
### OrderedDict
################################################################################

class OrderedDict(dict):
    __mm_tpname__ = 'collections.OrderedDict'

    def __init__(self, other=(), /, **kwds):
        self.update(other, **kwds)

    def popitem(self, last=True):
        b = _backing(self)
        if not b:
            raise KeyError('dictionary is empty')
        if last:
            return b.popitem()
        key = next(iter(b))
        return key, b.pop(key)

    def move_to_end(self, key, last=True):
        b = _backing(self)
        value = b.pop(key)
        if last:
            b[key] = value
        else:
            rest = list(b.items())
            b.clear()
            b[key] = value
            b.update(rest)

    def __repr__(self):
        if not self:
            return f'{self.__class__.__name__}()'
        if _repr_enter(self):
            return '...'
        try:
            body = repr(dict(_backing(self).items()))
        finally:
            _repr_leave(self)
        return f'{self.__class__.__name__}({body})'

    def copy(self):
        return self.__class__(self)

    __copy__ = copy

    @classmethod
    def fromkeys(cls, iterable, value=None):
        self = cls()
        for key in iterable:
            self[key] = value
        return self

    def __eq__(self, other):
        if isinstance(other, OrderedDict):
            return list(_backing(self).items()) == list(_backing(other).items())
        if isinstance(other, dict):
            return _backing(self) == other
        return NotImplemented

    def __ne__(self, other):
        r = self.__eq__(other)
        if r is NotImplemented:
            return r
        return not r

    __hash__ = None

    def __ior__(self, other):
        self.update(other)
        return self

    def __or__(self, other):
        if not isinstance(other, dict):
            return NotImplemented
        new = self.__class__(self)
        new.update(other)
        return new

    def __ror__(self, other):
        if not isinstance(other, dict):
            return NotImplemented
        new = self.__class__(other)
        new.update(self)
        return new

    __class_getitem__ = classmethod(_class_getitem)


################################################################################
### Counter
################################################################################

def _count_elements(mapping, iterable):
    get = mapping.get
    for elem in iterable:
        mapping[elem] = get(elem, 0) + 1


class Counter(dict):

    def __init__(self, iterable=None, /, **kwds):
        self.update(iterable, **kwds)

    def __missing__(self, key):
        return 0

    def total(self):
        return sum(self.values())

    def most_common(self, n=None):
        if n is None:
            return sorted(self.items(), key=_second, reverse=True)
        import heapq
        return heapq.nlargest(n, self.items(), key=_second)

    def elements(self):
        for elem, count in self.items():
            for _ in range(count):
                yield elem

    @classmethod
    def fromkeys(cls, iterable, v=None):
        raise NotImplementedError(
            'Counter.fromkeys() is undefined.  Use Counter(iterable) instead.')

    def update(self, iterable=None, /, **kwds):
        if iterable is not None:
            if isinstance(iterable, dict) or hasattr(iterable, 'keys'):
                if self:
                    get = self.get
                    for elem in iterable.keys():
                        self[elem] = iterable[elem] + get(elem, 0)
                else:
                    dict.update(self, iterable)
            else:
                _count_elements(self, iterable)
        if kwds:
            self.update(kwds)

    def subtract(self, iterable=None, /, **kwds):
        if iterable is not None:
            get = self.get
            if isinstance(iterable, dict) or hasattr(iterable, 'keys'):
                for elem in iterable.keys():
                    self[elem] = get(elem, 0) - iterable[elem]
            else:
                for elem in iterable:
                    self[elem] = get(elem, 0) - 1
        if kwds:
            self.subtract(kwds)

    def copy(self):
        return self.__class__(self)

    __copy__ = copy

    def __delitem__(self, elem):
        b = _backing(self)
        if elem in b:
            del b[elem]

    def __repr__(self):
        if not self:
            return f'{self.__class__.__name__}()'
        try:
            d = dict(self.most_common())
        except TypeError:
            d = dict(_backing(self))
        return f'{self.__class__.__name__}({d!r})'

    def __eq__(self, other):
        if not isinstance(other, Counter):
            return NotImplemented
        return all(self[e] == other[e] for c in (self, other) for e in c)

    def __ne__(self, other):
        if not isinstance(other, Counter):
            return NotImplemented
        return not self == other

    def __le__(self, other):
        if not isinstance(other, Counter):
            return NotImplemented
        return all(self[e] <= other[e] for c in (self, other) for e in c)

    def __lt__(self, other):
        if not isinstance(other, Counter):
            return NotImplemented
        return self <= other and self != other

    def __ge__(self, other):
        if not isinstance(other, Counter):
            return NotImplemented
        return all(self[e] >= other[e] for c in (self, other) for e in c)

    def __gt__(self, other):
        if not isinstance(other, Counter):
            return NotImplemented
        return self >= other and self != other

    __hash__ = None

    def __add__(self, other):
        if not isinstance(other, Counter):
            return NotImplemented
        result = Counter()
        for elem, count in self.items():
            newcount = count + other[elem]
            if newcount > 0:
                result[elem] = newcount
        for elem, count in other.items():
            if elem not in self and count > 0:
                result[elem] = count
        return result

    def __sub__(self, other):
        if not isinstance(other, Counter):
            return NotImplemented
        result = Counter()
        for elem, count in self.items():
            newcount = count - other[elem]
            if newcount > 0:
                result[elem] = newcount
        for elem, count in other.items():
            if elem not in self and count < 0:
                result[elem] = 0 - count
        return result

    def __or__(self, other):
        if not isinstance(other, Counter):
            return NotImplemented
        result = Counter()
        for elem, count in self.items():
            other_count = other[elem]
            newcount = other_count if count < other_count else count
            if newcount > 0:
                result[elem] = newcount
        for elem, count in other.items():
            if elem not in self and count > 0:
                result[elem] = count
        return result

    def __and__(self, other):
        if not isinstance(other, Counter):
            return NotImplemented
        result = Counter()
        for elem, count in self.items():
            other_count = other[elem]
            newcount = count if count < other_count else other_count
            if newcount > 0:
                result[elem] = newcount
        return result

    def __pos__(self):
        result = Counter()
        for elem, count in self.items():
            if count > 0:
                result[elem] = count
        return result

    def __neg__(self):
        result = Counter()
        for elem, count in self.items():
            if count < 0:
                result[elem] = 0 - count
        return result

    def _keep_positive(self):
        nonpositive = [elem for elem, count in self.items() if not count > 0]
        for elem in nonpositive:
            del self[elem]
        return self

    def __iadd__(self, other):
        for elem, count in other.items():
            self[elem] += count
        return self._keep_positive()

    def __isub__(self, other):
        for elem, count in other.items():
            self[elem] -= count
        return self._keep_positive()

    def __ior__(self, other):
        for elem, other_count in other.items():
            count = self[elem]
            if other_count > count:
                self[elem] = other_count
        return self._keep_positive()

    def __iand__(self, other):
        for elem, count in self.items():
            other_count = other[elem]
            if other_count < count:
                self[elem] = other_count
        return self._keep_positive()

    __class_getitem__ = classmethod(_class_getitem)


def _second(item):
    return item[1]


################################################################################
### ChainMap
################################################################################

class ChainMap:
    __class_getitem__ = classmethod(_class_getitem)
    __abc_tpflags__ = 1 << 6  # a mapping to `match`

    def __init__(self, *maps):
        self.maps = list(maps) or [{}]

    def __missing__(self, key):
        raise KeyError(key)

    def __getitem__(self, key):
        for mapping in self.maps:
            if key in mapping:
                return mapping[key]
        return self.__missing__(key)

    def get(self, key, default=None):
        return self[key] if key in self else default

    def __len__(self):
        return len(set().union(*self.maps))

    def __iter__(self):
        d = {}
        for mapping in reversed(self.maps):
            for k in mapping:
                d[k] = None
        return iter(d)

    def __contains__(self, key):
        return any(key in m for m in self.maps)

    def __bool__(self):
        return any(self.maps)

    def __repr__(self):
        return f'{self.__class__.__name__}({", ".join(map(repr, self.maps))})'

    def keys(self):
        from collections.abc import KeysView
        return KeysView(self)

    def items(self):
        from collections.abc import ItemsView
        return ItemsView(self)

    def values(self):
        from collections.abc import ValuesView
        return ValuesView(self)

    def __eq__(self, other):
        if isinstance(other, ChainMap):
            return dict(self.items()) == dict(other.items())
        if isinstance(other, dict):
            return dict(self.items()) == other
        return NotImplemented

    @classmethod
    def fromkeys(cls, iterable, value=None, /):
        return cls(dict.fromkeys(iterable, value))

    def copy(self):
        return self.__class__(self.maps[0].copy(), *self.maps[1:])

    __copy__ = copy

    def new_child(self, m=None, **kwargs):
        if m is None:
            m = kwargs
        elif kwargs:
            m.update(kwargs)
        return self.__class__(m, *self.maps)

    @property
    def parents(self):
        return self.__class__(*self.maps[1:])

    def __setitem__(self, key, value):
        self.maps[0][key] = value

    def __delitem__(self, key):
        try:
            del self.maps[0][key]
        except KeyError:
            raise KeyError(f'Key not found in the first mapping: {key!r}')

    def popitem(self):
        try:
            return self.maps[0].popitem()
        except KeyError:
            raise KeyError('No keys found in the first mapping.')

    def pop(self, key, *args):
        try:
            return self.maps[0].pop(key, *args)
        except KeyError:
            raise KeyError(f'Key not found in the first mapping: {key!r}')

    def clear(self):
        self.maps[0].clear()

    def setdefault(self, key, default=None):
        if key not in self:
            self[key] = default
        return self[key]

    def update(self, other=(), /, **kwds):
        self.maps[0].update(other, **kwds)

    def __ior__(self, other):
        self.maps[0].update(other)
        return self

    def __or__(self, other):
        if not _is_mapping(other):
            return NotImplemented
        m = self.copy()
        m.maps[0].update(other)
        return m

    def __ror__(self, other):
        if not _is_mapping(other):
            return NotImplemented
        m = dict(other)
        for child in reversed(self.maps):
            m.update(child)
        return self.__class__(m)


def _is_mapping(x):
    from collections.abc import Mapping
    return isinstance(x, Mapping)


################################################################################
### UserDict, UserList, UserString
################################################################################

class UserDict:
    __class_getitem__ = classmethod(_class_getitem)
    __abc_tpflags__ = 1 << 6  # a mapping to `match`

    def __init__(self, dict=None, /, **kwargs):
        self.data = {}
        if dict is not None:
            self.update(dict)
        if kwargs:
            self.update(kwargs)

    def __len__(self):
        return len(self.data)

    def __getitem__(self, key):
        if key in self.data:
            return self.data[key]
        if hasattr(self.__class__, '__missing__'):
            return self.__class__.__missing__(self, key)
        raise KeyError(key)

    def __setitem__(self, key, item):
        self.data[key] = item

    def __delitem__(self, key):
        del self.data[key]

    def __iter__(self):
        return iter(self.data)

    def __contains__(self, key):
        return key in self.data

    def get(self, key, default=None):
        if key in self:
            return self[key]
        return default

    def keys(self):
        from collections.abc import KeysView
        return KeysView(self)

    def items(self):
        from collections.abc import ItemsView
        return ItemsView(self)

    def values(self):
        from collections.abc import ValuesView
        return ValuesView(self)

    def update(self, other=(), /, **kwds):
        if hasattr(other, 'keys'):
            for key in other.keys():
                self[key] = other[key]
        else:
            for key, value in other:
                self[key] = value
        for key, value in kwds.items():
            self[key] = value

    def pop(self, key, *args):
        if len(args) > 1:
            raise TypeError(f'pop expected at most 2 arguments, got {len(args) + 1}')
        try:
            value = self[key]
        except KeyError:
            if not args:
                raise
            return args[0]
        del self[key]
        return value

    def popitem(self):
        try:
            key = next(iter(self))
        except StopIteration:
            raise KeyError from None
        value = self[key]
        del self[key]
        return key, value

    def clear(self):
        try:
            while True:
                self.popitem()
        except KeyError:
            pass

    def setdefault(self, key, default=None):
        try:
            return self[key]
        except KeyError:
            self[key] = default
        return default

    def __repr__(self):
        return repr(self.data)

    def __eq__(self, other):
        if isinstance(other, UserDict):
            other = other.data
        return self.data == other

    def __or__(self, other):
        if isinstance(other, UserDict):
            return self.__class__(self.data | other.data)
        if isinstance(other, dict):
            return self.__class__(self.data | other)
        return NotImplemented

    def __ror__(self, other):
        if isinstance(other, UserDict):
            return self.__class__(other.data | self.data)
        if isinstance(other, dict):
            return self.__class__(other | self.data)
        return NotImplemented

    def __ior__(self, other):
        if isinstance(other, UserDict):
            self.data |= other.data
        else:
            self.data |= other
        return self

    def __copy__(self):
        inst = self.__class__.__new__(self.__class__)
        inst.__dict__.update(self.__dict__)
        inst.__dict__['data'] = self.__dict__['data'].copy()
        return inst

    def copy(self):
        if self.__class__ is UserDict:
            return UserDict(self.data.copy())
        data = self.data
        try:
            self.data = {}
            c = self.__copy__()
        finally:
            self.data = data
        c.update(self)
        return c

    @classmethod
    def fromkeys(cls, iterable, value=None):
        d = cls()
        for key in iterable:
            d[key] = value
        return d


class UserList:
    __class_getitem__ = classmethod(_class_getitem)
    __abc_tpflags__ = 1 << 5  # a sequence to `match`

    def __init__(self, initlist=None):
        self.data = []
        if initlist is not None:
            if type(initlist) == type(self.data):
                self.data[:] = initlist
            elif isinstance(initlist, UserList):
                self.data[:] = initlist.data[:]
            else:
                self.data = list(initlist)

    def __repr__(self):
        return repr(self.data)

    def __cast(self, other):
        return other.data if isinstance(other, UserList) else other

    def __lt__(self, other):
        return self.data < self.__cast(other)

    def __le__(self, other):
        return self.data <= self.__cast(other)

    def __eq__(self, other):
        return self.data == self.__cast(other)

    def __gt__(self, other):
        return self.data > self.__cast(other)

    def __ge__(self, other):
        return self.data >= self.__cast(other)

    def __contains__(self, item):
        return item in self.data

    def __len__(self):
        return len(self.data)

    def __getitem__(self, i):
        if isinstance(i, slice):
            return self.__class__(self.data[i])
        return self.data[i]

    def __setitem__(self, i, item):
        self.data[i] = item

    def __delitem__(self, i):
        del self.data[i]

    def __iter__(self):
        return iter(self.data)

    def __add__(self, other):
        if isinstance(other, UserList):
            return self.__class__(self.data + other.data)
        elif isinstance(other, type(self.data)):
            return self.__class__(self.data + other)
        return self.__class__(self.data + list(other))

    def __radd__(self, other):
        if isinstance(other, UserList):
            return self.__class__(other.data + self.data)
        elif isinstance(other, type(self.data)):
            return self.__class__(other + self.data)
        return self.__class__(list(other) + self.data)

    def __iadd__(self, other):
        if isinstance(other, UserList):
            self.data += other.data
        elif isinstance(other, type(self.data)):
            self.data += other
        else:
            self.data += list(other)
        return self

    def __mul__(self, n):
        return self.__class__(self.data * n)

    __rmul__ = __mul__

    def __imul__(self, n):
        self.data *= n
        return self

    def __copy__(self):
        inst = self.__class__.__new__(self.__class__)
        inst.__dict__.update(self.__dict__)
        inst.__dict__['data'] = self.__dict__['data'][:]
        return inst

    def append(self, item):
        self.data.append(item)

    def insert(self, i, item):
        self.data.insert(i, item)

    def pop(self, i=-1):
        return self.data.pop(i)

    def remove(self, item):
        self.data.remove(item)

    def clear(self):
        self.data.clear()

    def copy(self):
        return self.__class__(self)

    def count(self, item):
        return self.data.count(item)

    def index(self, item, *args):
        return self.data.index(item, *args)

    def reverse(self):
        self.data.reverse()

    def sort(self, /, *args, **kwds):
        self.data.sort(*args, **kwds)

    def extend(self, other):
        if isinstance(other, UserList):
            self.data.extend(other.data)
        else:
            self.data.extend(other)


class UserString:
    __class_getitem__ = classmethod(_class_getitem)

    def __init__(self, seq):
        if isinstance(seq, str):
            self.data = seq
        elif isinstance(seq, UserString):
            self.data = seq.data[:]
        else:
            self.data = str(seq)

    def __str__(self):
        return str(self.data)

    def __repr__(self):
        return repr(self.data)

    def __int__(self):
        return int(self.data)

    def __float__(self):
        return float(self.data)

    def __complex__(self):
        return complex(self.data)

    def __getnewargs__(self):
        return (self.data[:],)

    def __eq__(self, string):
        if isinstance(string, UserString):
            return self.data == string.data
        return self.data == string

    def __lt__(self, string):
        if isinstance(string, UserString):
            return self.data < string.data
        return self.data < string

    def __le__(self, string):
        if isinstance(string, UserString):
            return self.data <= string.data
        return self.data <= string

    def __gt__(self, string):
        if isinstance(string, UserString):
            return self.data > string.data
        return self.data > string

    def __ge__(self, string):
        if isinstance(string, UserString):
            return self.data >= string.data
        return self.data >= string

    def __hash__(self):
        return hash(self.data)

    def __contains__(self, char):
        if isinstance(char, UserString):
            char = char.data
        return char in self.data

    def __len__(self):
        return len(self.data)

    def __getitem__(self, index):
        return self.__class__(self.data[index])

    def __add__(self, other):
        if isinstance(other, UserString):
            return self.__class__(self.data + other.data)
        elif isinstance(other, str):
            return self.__class__(self.data + other)
        return self.__class__(self.data + str(other))

    def __radd__(self, other):
        if isinstance(other, str):
            return self.__class__(other + self.data)
        return self.__class__(str(other) + self.data)

    def __mul__(self, n):
        return self.__class__(self.data * n)

    __rmul__ = __mul__

    def __mod__(self, args):
        return self.__class__(self.data % args)

    def __rmod__(self, template):
        return self.__class__(str(template) % self)

    def encode(self, encoding='utf-8', errors='strict'):
        encoding = 'utf-8' if encoding is None else encoding
        errors = 'strict' if errors is None else errors
        return self.data.encode(encoding, errors)

    maketrans = str.maketrans

    def __getattr__(self, name):
        # The other str methods, as CPython's UserString spells each out:
        # UserString arguments unwrap, and the methods that build a new
        # string return the class.
        if name.startswith('__'):
            raise AttributeError(f"'{type(self).__name__}' object has no attribute '{name}'")
        attr = getattr(self.data, name)
        if not callable(attr):
            return attr
        wrap = name in _USERSTRING_WRAP

        def call(*args, **kwargs):
            args = [a.data if isinstance(a, UserString) else a for a in args]
            r = attr(*args, **kwargs)
            return self.__class__(r) if wrap else r
        return call


_USERSTRING_WRAP = frozenset((
    'capitalize', 'casefold', 'center', 'expandtabs', 'ljust', 'lower', 'lstrip', 'removeprefix',
    'removesuffix', 'replace', 'rjust', 'rstrip', 'strip', 'swapcase', 'title', 'translate', 'upper',
    'zfill'))


_MISSING = object()


def __getattr__(name):
    # CPython's collections imports `collections.abc` itself, so
    # `collections.abc.Mapping` works after `import collections`. Here the
    # first such lookup imports it: `abc` would double the import's cost.
    import collections
    if name == 'abc':
        import collections.abc
        return collections.abc
    raise AttributeError(f"module 'collections' has no attribute '{name}'", name=name, obj=collections)
