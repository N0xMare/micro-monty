# typing.Callable spreads its parameter list into `__args__` before the
# result, and its repr and get_args gather the list back: it reprd as
# `typing.Callable[[<class 'int'>], str]` and kept the list in `__args__`,
# which made the alias unhashable (difffuzz, the 0.0.4 typing block).
# Beside it: NoneType in an alias's repr, `__parameters__` once each,
# and get_origin of an alias being the class it stands for.
import typing
from typing import Callable, get_args, get_origin

T = typing.TypeVar('T')
P = typing.ParamSpec('P')
forms = [
    Callable[[int], str],
    Callable[[], int],
    Callable[[int, str], None],
    Callable[..., int],
    Callable[..., T],
    Callable[[T], list[T]],
    Callable[[Callable[[int], str]], Callable[..., bytes]],
    Callable[[Callable[[Callable[[], int]], None], typing.List[int]], dict[str, int]],
    Callable[[typing.Optional[str], tuple[int, ...]], typing.Dict[str, None]],
    Callable[int, str],
    Callable[P, int],
    Callable[typing.Concatenate[int, P], int],
]
for c in forms:
    print(repr(c))
    print('  ', c.__args__, get_args(c), get_origin(c))
a = Callable[[int], str]
print(a == Callable[[int], str], a == Callable[[str], str], a == Callable[int, str],
      hash(a) == hash(Callable[[int], str]), len({a, Callable[[int], str], Callable[..., str]}))
for build in (lambda: Callable[[int]], lambda: Callable[int], lambda: Callable[[int], str, bytes]):
    try:
        build()
    except TypeError as e:
        print('TypeError', e)
print(get_origin(Callable), get_args(Callable), repr(Callable))
print(repr(typing.List[None]), repr(typing.Tuple[int, None]), repr(typing.Type[None]))
print(typing.Dict[T, T].__parameters__, Callable[[T, int, T], T].__parameters__)
for alias in (typing.Mapping[str, int], typing.Iterable[int], typing.AbstractSet[int],
              typing.Pattern[str], typing.List[int],
              typing.Annotated[int, 'x'], typing.ClassVar[int]):
    print(get_origin(alias), get_args(alias))
print([get_origin(f) for f in (typing.Sequence, typing.Deque, typing.Any, typing.ClassVar)])
print(isinstance({1}, typing.AbstractSet), isinstance([1], typing.AbstractSet),
      issubclass(frozenset, typing.AbstractSet))
