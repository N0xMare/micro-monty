# typing at run time: annotations are not evaluated, the markers repr as in
# CPython, and NamedTuple/TypedDict/Generic/TypeVar work as classes.
from typing import (Any, Callable, ClassVar, Dict, Generic, List, NamedTuple,
                    Optional, Protocol, TypedDict, TypeVar, Union, cast,
                    overload, TYPE_CHECKING)
import typing

T = TypeVar('T')
K = TypeVar('K', bound=int, covariant=True)
print(T, K, Any, Optional, List[int], Dict[str, List[int]], Callable)
print(Optional[int], Union[int, str], Union[int, int], TYPE_CHECKING)


class Box(Generic[T]):
    def __init__(self, item: T) -> None:
        self.item = item


print(Box(3).item, Box[int](4).item)


class Employee(NamedTuple):
    name: str
    id: int = 3


e = Employee('ada')
print(e, e.id, e._fields, e._replace(id=7), isinstance(e, tuple), Employee._field_defaults)
a, b = Employee('bob', 1)
print(a, b)


class Movie(TypedDict):
    title: str
    year: int


m = Movie(title='x', year=1)
print(m, type(m).__name__, Movie.__required_keys__ == {'title', 'year'})


class Greeter(Protocol):
    def greet(self) -> str: ...


class En:
    def greet(self) -> str:
        return 'hi'


print(En().greet(), cast(int, 'not checked'), typing.get_origin(List[int]), typing.get_args(Dict[str, int]))


@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...
def f(x):
    return x


print(f(1), f('a'))
try:
    typing.assert_never(1)
except AssertionError as e:
    print('AssertionError', e)
