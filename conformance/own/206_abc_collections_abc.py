# abc.ABCMeta (abstract methods, register, __subclasshook__, caches) and
# collections.abc: builtins and registered/subclassed types, mixin methods,
# match on Mapping/Sequence subclasses.
from abc import ABC, ABCMeta, abstractmethod, get_cache_token
import abc


class Shape(ABC):
    @abstractmethod
    def area(self): ...

    @abstractmethod
    def name(self): ...

    @property
    @abc.abstractmethod
    def sides(self): ...


class Square(Shape):
    def __init__(self, s):
        self.s = s

    def area(self):
        return self.s ** 2

    def name(self):
        return 'square'

    @property
    def sides(self):
        return 4


print(sorted(Shape.__abstractmethods__), Square.__abstractmethods__, type(Shape), isinstance(Shape, ABCMeta))
print(Square(3).area(), Square(2).sides, isinstance(Square(1), Shape), issubclass(Square, Shape), issubclass(int, Shape))
for c in (Shape,):
    try:
        c()
    except TypeError as e:
        print('TypeError', e)


class Half(Shape):
    def area(self):
        return 0

    sides = 3


try:
    Half()
except TypeError as e:
    print('TypeError', e)


class Drawable(metaclass=ABCMeta):
    @classmethod
    def __subclasshook__(cls, C):
        if cls is Drawable:
            return hasattr(C, 'draw')
        return NotImplemented


class Pic:
    def draw(self):
        return 'drawn'


print(isinstance(Pic(), Drawable), issubclass(int, Drawable), isinstance(3, Drawable))


class Base(ABC):
    pass


tok = get_cache_token()
Base.register(int)
r = Base.register(str)
print(r, isinstance(3, Base), issubclass(bool, Base), isinstance('x', Base), isinstance(3.0, Base), get_cache_token() > tok)


@Base.register
class Other:
    pass


print(Other.__name__, isinstance(Other(), Base), Base in Other.__mro__)


class Sub(Base):
    pass


print(isinstance(Sub(), Base), issubclass(Sub, Base), issubclass(int, Sub))
try:
    Base.register(5)
except TypeError as e:
    print('TypeError', e)
try:
    issubclass(5, Base)
except TypeError as e:
    print('TypeError', e)
try:
    Sub.register(Base)
except RuntimeError as e:
    print('RuntimeError', e)


class Meta2(ABCMeta):
    pass


class WithMeta(metaclass=Meta2):
    @abstractmethod
    def f(self): ...


try:
    WithMeta()
except TypeError as e:
    print('TypeError', e)


class Impl(WithMeta):
    def f(self):
        return 'f'


print(Impl().f(), type(WithMeta).__name__)


def g(self):
    return 1


class Late(ABC):
    @abstractmethod
    def g(self): ...


Late.g = g
abc.update_abstractmethods(Late)
print(Late().g(), Late.__abstractmethods__)


import collections.abc
from collections.abc import (Mapping, MutableMapping, Sequence, MutableSequence, Set, MutableSet,
                             Iterable, Iterator, Hashable, Sized, Container, Collection, Callable,
                             Generator, Reversible, KeysView, ItemsView, ValuesView, Awaitable, Coroutine)
from collections import deque, OrderedDict, defaultdict, Counter, UserDict, UserList, ChainMap


def gen():
    yield 1


async def co():
    return 1


vals = [[], (), '', b'', {}, set(), frozenset(), range(3), 1, 1.5, None, True, len, gen, gen(),
        iter([]), iter({}), {}.keys(), {}.values(), {}.items(), deque(), OrderedDict(),
        defaultdict(int), Counter(), UserDict(), UserList(), ChainMap(), reversed([]), zip(), enumerate([])]
abcs = [Mapping, MutableMapping, Sequence, MutableSequence, Set, MutableSet, Iterable, Iterator,
        Hashable, Sized, Container, Collection, Callable, Generator, Reversible, KeysView, ItemsView, ValuesView]
for v in vals:
    print(type(v).__name__, ''.join('1' if isinstance(v, a) else '.' for a in abcs))
c = co()
print(isinstance(c, Awaitable), isinstance(c, Coroutine))
import asyncio
asyncio.run(c)


class MyMap(Mapping):
    def __init__(self, **kw):
        self._d = kw

    def __getitem__(self, k):
        return self._d[k]

    def __iter__(self):
        return iter(self._d)

    def __len__(self):
        return len(self._d)


m = MyMap(a=1, b=2)
print(isinstance(m, Mapping), isinstance(m, MutableMapping), 'a' in m, m.get('z', 0), list(m.keys()), list(m.items()), list(m.values()))
print(m == {'a': 1, 'b': 2}, m.keys() & {'a'}, len(m), dict(m), isinstance(m.keys(), Set), m.keys() == {'a', 'b'})
match m:
    case {'a': x, **rest}:
        print('matched', x, rest)


class MySeq(Sequence):
    def __init__(self, *xs):
        self._xs = xs

    def __getitem__(self, i):
        return self._xs[i]

    def __len__(self):
        return len(self._xs)


s = MySeq(1, 2, 3, 2)
print(list(s), 2 in s, s.index(2), s.count(2), list(reversed(s)), isinstance(s, Reversible))
match s:
    case [a, *rest]:
        print('seq', a, rest)
try:
    s.index(9)
except ValueError as e:
    print('ValueError', repr(e))


class Bag(MutableSet):
    def __init__(self, it=()):
        self._s = list(it)

    def __contains__(self, x):
        return x in self._s

    def __iter__(self):
        return iter(self._s)

    def __len__(self):
        return len(self._s)

    def add(self, x):
        if x not in self._s:
            self._s.append(x)

    def discard(self, x):
        if x in self._s:
            self._s.remove(x)


b = Bag([1, 2, 3])
b |= {4}
b -= {1}
print(sorted(b), sorted(b & {2, 3, 9}), sorted(b | [7]), b <= Bag([2, 3, 4, 5]), b == {2, 3, 4}, b._hash() == hash(frozenset({2, 3, 4})))


class MList(MutableSequence):
    def __init__(self):
        self._l = []

    def __getitem__(self, i):
        return self._l[i]

    def __setitem__(self, i, v):
        self._l[i] = v

    def __delitem__(self, i):
        del self._l[i]

    def __len__(self):
        return len(self._l)

    def insert(self, i, v):
        self._l.insert(i, v)


ml = MList()
ml.append(1)
ml.extend([2, 3])
ml += [4]
ml.reverse()
print(list(ml), ml.pop(), ml.pop(0), list(ml))
ml.remove(3)
print(list(ml))
try:
    Sequence()
except TypeError as e:
    print('TypeError', e)


class Half(Mapping):
    def __getitem__(self, k):
        return 1


try:
    Half()
except TypeError as e:
    print('TypeError', e)


class Pony:
    def __len__(self):
        return 0

    def __call__(self):
        pass

    __hash__ = None


print(isinstance(Pony(), Sized), isinstance(Pony(), Callable), isinstance(Pony(), Hashable), issubclass(Pony, Iterable))
print(collections.abc.Sequence.__name__, issubclass(list, Sequence), issubclass(dict, Mapping), issubclass(str, MutableSequence))
