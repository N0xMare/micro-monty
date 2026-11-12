# collections.abc, adapted from CPython 3.14's Lib/_collections_abc.py
# (PSF license, see THIRD_PARTY.md). Builtin types have no `__dict__`
# here, so the one-trick ponies' subclass hooks read a table of the
# methods CPython's builtin types define; `__class_getitem__` returns the
# class (no `types.GenericAlias`); no ByteString, async generators or
# `__slots__`.

from abc import ABCMeta, abstractmethod
import collections as _c

__all__ = ["Awaitable", "Coroutine",
           "AsyncIterable", "AsyncIterator", "AsyncGenerator",
           "Hashable", "Iterable", "Iterator", "Generator", "Reversible",
           "Sized", "Container", "Callable", "Collection",
           "Set", "MutableSet",
           "Mapping", "MutableMapping",
           "MappingView", "KeysView", "ItemsView", "ValuesView",
           "Sequence", "MutableSequence", "Buffer",
           ]

bytes_iterator = type(iter(b''))
bytearray_iterator = type(iter(bytearray()))
dict_keyiterator = type(iter({}.keys()))
dict_valueiterator = type(iter({}.values()))
dict_itemiterator = type(iter({}.items()))
list_iterator = type(iter([]))
list_reverseiterator = type(iter(reversed([])))
range_iterator = type(iter(range(0)))
set_iterator = type(iter(set()))
str_iterator = type(iter(""))
tuple_iterator = type(iter(()))
zip_iterator = type(iter(zip()))
dict_keys = type({}.keys())
dict_values = type({}.values())
dict_items = type({}.items())
generator = type((lambda: (yield))())


async def _coro():
    pass


coroutine = type(_coro())
del _coro

# What the builtin types' `__dict__` holds that a hook looks for (`-` marks
# `__hash__ = None`).
_SEQ = '__iter__ __len__ __contains__ __getitem__ '
_ITER = '__iter__ __next__'
_BUILTIN = {}
for _t, _m in ((object, '__hash__'), (int, '__hash__'), (float, '__hash__'),
               (complex, '__hash__'), (type(None), '__hash__'),
               (str, _SEQ + '__hash__'), (bytes, _SEQ + '__hash__ __buffer__'),
               (tuple, _SEQ + '__hash__'), (list, _SEQ + '__reversed__ -'),
               (range, _SEQ + '__reversed__ __hash__'),
               (dict, _SEQ + '__reversed__ -'), (set, '__iter__ __len__ __contains__ -'),
               (frozenset, '__iter__ __len__ __contains__ __hash__'),
               (dict_keys, '__iter__ __len__ __contains__ __reversed__ -'),
               (dict_items, '__iter__ __len__ __contains__ __reversed__ -'),
               (dict_values, '__iter__ __len__ __reversed__'),
               (type, '__call__'), (type(len), '__call__ __hash__'),
               (type(_BUILTIN.get), '__call__ __hash__'),
               (type(lambda: 0), '__call__'),
               (generator, _ITER + ' send throw close'),
               (coroutine, '__await__ send throw close'),
               (bytes_iterator, _ITER), (bytearray_iterator, _ITER),
               (dict_keyiterator, _ITER),
               (dict_valueiterator, _ITER), (dict_itemiterator, _ITER),
               (list_iterator, _ITER), (list_reverseiterator, _ITER),
               (range_iterator, _ITER), (set_iterator, _ITER), (str_iterator, _ITER),
               (type(iter('\xe9')), _ITER),
               (tuple_iterator, _ITER), (zip_iterator, _ITER),
               (type(enumerate([])), _ITER)):
    _BUILTIN[_t] = _m.split()
del _t, _m


def _check_methods(C, *methods):
    mro = C.__mro__
    for method in methods:
        for B in mro:
            own = _BUILTIN.get(B)
            if own is not None:
                if method in own:
                    break
                if method == '__hash__' and '-' in own:
                    return NotImplemented
                continue
            d = getattr(B, '__dict__', None)
            if d is not None and method in d:
                if d[method] is None:
                    return NotImplemented
                break
        else:
            return NotImplemented
    return True


def _alias(cls, item):
    return cls


class Hashable(metaclass=ABCMeta):

    @abstractmethod
    def __hash__(self):
        return 0

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Hashable:
            return _check_methods(C, "__hash__")
        return NotImplemented


class Awaitable(metaclass=ABCMeta):

    @abstractmethod
    def __await__(self):
        yield

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Awaitable:
            return _check_methods(C, "__await__")
        return NotImplemented

    __class_getitem__ = classmethod(_alias)


class Coroutine(Awaitable):

    @abstractmethod
    def send(self, value):
        raise StopIteration

    @abstractmethod
    def throw(self, typ, val=None, tb=None):
        if val is None:
            if tb is None:
                raise typ
            val = typ()
        raise val

    def close(self):
        try:
            self.throw(GeneratorExit)
        except (GeneratorExit, StopIteration):
            pass
        else:
            raise RuntimeError("coroutine ignored GeneratorExit")

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Coroutine:
            return _check_methods(C, '__await__', 'send', 'throw', 'close')
        return NotImplemented


Coroutine.register(coroutine)


class AsyncIterable(metaclass=ABCMeta):

    @abstractmethod
    def __aiter__(self):
        return AsyncIterator()

    @classmethod
    def __subclasshook__(cls, C):
        if cls is AsyncIterable:
            return _check_methods(C, "__aiter__")
        return NotImplemented

    __class_getitem__ = classmethod(_alias)


class AsyncIterator(AsyncIterable):

    @abstractmethod
    async def __anext__(self):
        raise StopAsyncIteration

    def __aiter__(self):
        return self

    @classmethod
    def __subclasshook__(cls, C):
        if cls is AsyncIterator:
            return _check_methods(C, "__anext__", "__aiter__")
        return NotImplemented


class AsyncGenerator(AsyncIterator):

    async def __anext__(self):
        return await self.asend(None)

    @abstractmethod
    async def asend(self, value):
        raise StopAsyncIteration

    @abstractmethod
    async def athrow(self, typ, val=None, tb=None):
        if val is None:
            if tb is None:
                raise typ
            val = typ()
        raise val

    async def aclose(self):
        try:
            await self.athrow(GeneratorExit)
        except (GeneratorExit, StopAsyncIteration):
            pass
        else:
            raise RuntimeError("asynchronous generator ignored GeneratorExit")

    @classmethod
    def __subclasshook__(cls, C):
        if cls is AsyncGenerator:
            return _check_methods(C, '__aiter__', '__anext__', 'asend', 'athrow', 'aclose')
        return NotImplemented


class Iterable(metaclass=ABCMeta):

    @abstractmethod
    def __iter__(self):
        while False:
            yield None

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Iterable:
            return _check_methods(C, "__iter__")
        return NotImplemented

    __class_getitem__ = classmethod(_alias)


class Iterator(Iterable):

    @abstractmethod
    def __next__(self):
        raise StopIteration

    def __iter__(self):
        return self

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Iterator:
            return _check_methods(C, '__iter__', '__next__')
        return NotImplemented


for _t in (bytes_iterator, bytearray_iterator, dict_keyiterator, dict_valueiterator,
           dict_itemiterator, list_iterator, list_reverseiterator, range_iterator, set_iterator,
           str_iterator, type(iter('\xe9')), tuple_iterator, zip_iterator):
    Iterator.register(_t)
del _t


class Reversible(Iterable):

    @abstractmethod
    def __reversed__(self):
        while False:
            yield None

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Reversible:
            return _check_methods(C, "__reversed__", "__iter__")
        return NotImplemented


class Generator(Iterator):

    def __next__(self):
        return self.send(None)

    @abstractmethod
    def send(self, value):
        raise StopIteration

    @abstractmethod
    def throw(self, typ, val=None, tb=None):
        if val is None:
            if tb is None:
                raise typ
            val = typ()
        raise val

    def close(self):
        try:
            self.throw(GeneratorExit)
        except (GeneratorExit, StopIteration):
            pass
        else:
            raise RuntimeError("generator ignored GeneratorExit")

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Generator:
            return _check_methods(C, '__iter__', '__next__', 'send', 'throw', 'close')
        return NotImplemented


Generator.register(generator)


class Sized(metaclass=ABCMeta):

    @abstractmethod
    def __len__(self):
        return 0

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Sized:
            return _check_methods(C, "__len__")
        return NotImplemented


class Container(metaclass=ABCMeta):

    @abstractmethod
    def __contains__(self, x):
        return False

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Container:
            return _check_methods(C, "__contains__")
        return NotImplemented

    __class_getitem__ = classmethod(_alias)


class Collection(Sized, Iterable, Container):

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Collection:
            return _check_methods(C, "__len__", "__iter__", "__contains__")
        return NotImplemented


class Buffer(metaclass=ABCMeta):

    @abstractmethod
    def __buffer__(self, flags, /):
        raise NotImplementedError

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Buffer:
            return _check_methods(C, "__buffer__")
        return NotImplemented


class Callable(metaclass=ABCMeta):

    @abstractmethod
    def __call__(self, *args, **kwds):
        return False

    @classmethod
    def __subclasshook__(cls, C):
        if cls is Callable:
            return _check_methods(C, "__call__")
        return NotImplemented

    __class_getitem__ = classmethod(_alias)


class Set(Collection):

    def __le__(self, other):
        if not isinstance(other, Set):
            return NotImplemented
        if len(self) > len(other):
            return False
        for elem in self:
            if elem not in other:
                return False
        return True

    def __lt__(self, other):
        if not isinstance(other, Set):
            return NotImplemented
        return len(self) < len(other) and self.__le__(other)

    def __gt__(self, other):
        if not isinstance(other, Set):
            return NotImplemented
        return len(self) > len(other) and self.__ge__(other)

    def __ge__(self, other):
        if not isinstance(other, Set):
            return NotImplemented
        if len(self) < len(other):
            return False
        for elem in other:
            if elem not in self:
                return False
        return True

    def __eq__(self, other):
        if not isinstance(other, Set):
            return NotImplemented
        return len(self) == len(other) and self.__le__(other)

    @classmethod
    def _from_iterable(cls, it):
        return cls(it)

    def __and__(self, other):
        if not isinstance(other, Iterable):
            return NotImplemented
        return self._from_iterable(value for value in other if value in self)

    __rand__ = __and__

    def isdisjoint(self, other):
        for value in other:
            if value in self:
                return False
        return True

    def __or__(self, other):
        if not isinstance(other, Iterable):
            return NotImplemented
        return self._from_iterable(e for s in (self, other) for e in s)

    __ror__ = __or__

    def __sub__(self, other):
        if not isinstance(other, Set):
            if not isinstance(other, Iterable):
                return NotImplemented
            other = self._from_iterable(other)
        return self._from_iterable(value for value in self if value not in other)

    def __rsub__(self, other):
        if not isinstance(other, Set):
            if not isinstance(other, Iterable):
                return NotImplemented
            other = self._from_iterable(other)
        return self._from_iterable(value for value in other if value not in self)

    def __xor__(self, other):
        if not isinstance(other, Set):
            if not isinstance(other, Iterable):
                return NotImplemented
            other = self._from_iterable(other)
        return (self - other) | (other - self)

    __rxor__ = __xor__

    def _hash(self):
        MAX = 9223372036854775807
        MASK = 2 * MAX + 1
        n = len(self)
        h = 1927868237 * (n + 1)
        h &= MASK
        for x in self:
            hx = hash(x)
            h ^= (hx ^ (hx << 16) ^ 89869747) * 3644798167
            h &= MASK
        h ^= (h >> 11) ^ (h >> 25)
        h = h * 69069 + 907133923
        h &= MASK
        if h > MAX:
            h -= MASK + 1
        if h == -1:
            h = 590923713
        return h


Set.register(frozenset)


class MutableSet(Set):

    @abstractmethod
    def add(self, value):
        raise NotImplementedError

    @abstractmethod
    def discard(self, value):
        raise NotImplementedError

    def remove(self, value):
        if value not in self:
            raise KeyError(value)
        self.discard(value)

    def pop(self):
        it = iter(self)
        try:
            value = next(it)
        except StopIteration:
            raise KeyError from None
        self.discard(value)
        return value

    def clear(self):
        try:
            while True:
                self.pop()
        except KeyError:
            pass

    def __ior__(self, it):
        for value in it:
            self.add(value)
        return self

    def __iand__(self, it):
        for value in (self - it):
            self.discard(value)
        return self

    def __ixor__(self, it):
        if it is self:
            self.clear()
        else:
            if not isinstance(it, Set):
                it = self._from_iterable(it)
            for value in it:
                if value in self:
                    self.discard(value)
                else:
                    self.add(value)
        return self

    def __isub__(self, it):
        if it is self:
            self.clear()
        else:
            for value in it:
                self.discard(value)
        return self


MutableSet.register(set)


class Mapping(Collection):

    __abc_tpflags__ = 1 << 6  # Py_TPFLAGS_MAPPING

    @abstractmethod
    def __getitem__(self, key):
        raise KeyError

    def get(self, key, default=None):
        try:
            return self[key]
        except KeyError:
            return default

    def __contains__(self, key):
        try:
            self[key]
        except KeyError:
            return False
        else:
            return True

    def keys(self):
        return KeysView(self)

    def items(self):
        return ItemsView(self)

    def values(self):
        return ValuesView(self)

    def __eq__(self, other):
        if not isinstance(other, Mapping):
            return NotImplemented
        return dict(self.items()) == dict(other.items())

    __reversed__ = None


class MappingView(Sized):

    def __init__(self, mapping):
        self._mapping = mapping

    def __len__(self):
        return len(self._mapping)

    def __repr__(self):
        return '{0.__class__.__name__}({0._mapping!r})'.format(self)

    __class_getitem__ = classmethod(_alias)


class KeysView(MappingView, Set):

    @classmethod
    def _from_iterable(cls, it):
        return set(it)

    def __contains__(self, key):
        return key in self._mapping

    def __iter__(self):
        yield from self._mapping


KeysView.register(dict_keys)


class ItemsView(MappingView, Set):

    @classmethod
    def _from_iterable(cls, it):
        return set(it)

    def __contains__(self, item):
        key, value = item
        try:
            v = self._mapping[key]
        except KeyError:
            return False
        else:
            return v is value or v == value

    def __iter__(self):
        for key in self._mapping:
            yield (key, self._mapping[key])


ItemsView.register(dict_items)


class ValuesView(MappingView, Collection):

    def __contains__(self, value):
        for key in self._mapping:
            v = self._mapping[key]
            if v is value or v == value:
                return True
        return False

    def __iter__(self):
        for key in self._mapping:
            yield self._mapping[key]


ValuesView.register(dict_values)

_marker = object()


class MutableMapping(Mapping):

    @abstractmethod
    def __setitem__(self, key, value):
        raise KeyError

    @abstractmethod
    def __delitem__(self, key):
        raise KeyError

    def pop(self, key, default=_marker):
        try:
            value = self[key]
        except KeyError:
            if default is _marker:
                raise
            return default
        else:
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

    def update(self, other=(), /, **kwds):
        if isinstance(other, Mapping):
            for key in other:
                self[key] = other[key]
        elif hasattr(other, "keys"):
            for key in other.keys():
                self[key] = other[key]
        else:
            for key, value in other:
                self[key] = value
        for key, value in kwds.items():
            self[key] = value

    def setdefault(self, key, default=None):
        try:
            return self[key]
        except KeyError:
            self[key] = default
        return default


MutableMapping.register(dict)
MutableMapping.register(_c.UserDict)
MutableMapping.register(_c.ChainMap)


class Sequence(Reversible, Collection):

    __abc_tpflags__ = 1 << 5  # Py_TPFLAGS_SEQUENCE

    @abstractmethod
    def __getitem__(self, index):
        raise IndexError

    def __iter__(self):
        i = 0
        try:
            while True:
                v = self[i]
                yield v
                i += 1
        except IndexError:
            return

    def __contains__(self, value):
        for v in self:
            if v is value or v == value:
                return True
        return False

    def __reversed__(self):
        for i in reversed(range(len(self))):
            yield self[i]

    def index(self, value, start=0, stop=None):
        if start is not None and start < 0:
            start = max(len(self) + start, 0)
        if stop is not None and stop < 0:
            stop += len(self)
        i = start
        while stop is None or i < stop:
            try:
                v = self[i]
            except IndexError:
                break
            if v is value or v == value:
                return i
            i += 1
        raise ValueError

    def count(self, value):
        return sum(1 for v in self if v is value or v == value)


for _t in (tuple, str, bytes, range, _c.UserString):
    Sequence.register(_t)


class MutableSequence(Sequence):

    @abstractmethod
    def __setitem__(self, index, value):
        raise IndexError

    @abstractmethod
    def __delitem__(self, index):
        raise IndexError

    @abstractmethod
    def insert(self, index, value):
        raise IndexError

    def append(self, value):
        self.insert(len(self), value)

    def clear(self):
        try:
            while True:
                self.pop()
        except IndexError:
            pass

    def reverse(self):
        n = len(self)
        for i in range(n // 2):
            self[i], self[n - i - 1] = self[n - i - 1], self[i]

    def extend(self, values):
        if values is self:
            values = list(values)
        for v in values:
            self.append(v)

    def pop(self, index=-1):
        v = self[index]
        del self[index]
        return v

    def remove(self, value):
        del self[self.index(value)]

    def __iadd__(self, values):
        self.extend(values)
        return self


for _t in (list, _c.deque, _c.UserList):
    MutableSequence.register(_t)
del _t
