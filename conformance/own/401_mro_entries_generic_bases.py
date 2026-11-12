# PEP 560 `__mro_entries__`: a class statement whose base is not a class
# (a subscripted user generic, `List[int]`, `list[int]`, `typing.Mapping`,
# an object with `__mro_entries__`) uses what `__mro_entries__(bases)`
# returns and keeps the originals in `__orig_bases__`; typing's aliases add
# `Generic` as CPython's do.
from typing import Generic, TypeVar, List, Dict, NamedTuple, Protocol, Any
from dataclasses import dataclass, astuple
import typing
T = TypeVar("T"); S = TypeVar("S")
class Box(Generic[T]):
    def __init__(self, v): self.v = v
class IntBox(Box[int]):
    def get(self): return self.v + 1
print(IntBox(3).get(), IntBox.__bases__, IntBox.__orig_bases__, IntBox.__mro__)
@dataclass
class Base(Generic[T, S]):
    x: T
    y: S
@dataclass
class Derived(Base[int, T]):
    z: str
print(astuple(Derived[str](0, 'a', 'b')), Derived.__orig_bases__)
class L(List[int]):
    def total(self): return sum(self)
print(L([1, 2]).total(), L.__mro__)
class D(dict[str, int]): pass
print(D(a=1), D.__orig_bases__)
class M(typing.Mapping[str, int]):
    def __getitem__(self, k): return 1
    def __iter__(self): return iter("ab")
    def __len__(self): return 2
print(dict(M()))
class E:
    def __mro_entries__(self, bases): return (int,)
class F(E()): pass
print(F(5) + 1, F.__bases__)
class Bad:
    def __mro_entries__(self, bases): return [int]
try:
    class G(Bad()): pass
except TypeError as e: print(e)
class W(Box[int]):
    def m(self): return __class__.__name__, super().__init__ is not None
print(W(1).m())
try:
    class K(1): pass
except TypeError as e: print(type(e).__name__)
print(type('Z', (Box,), {}).__mro__[1].__name__)
class N(typing.List): pass
print(N.__mro__)
try:
    class Q2(typing.Literal[1]): pass
except TypeError as e: print(e)
class Q3(typing.Sequence[int]):
    def __getitem__(self, i): return [5, 6][i]
    def __len__(self): return 2
print(list(Q3()), Q3().index(6), [c.__name__ for c in Q3.__mro__])
