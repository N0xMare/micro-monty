# isinstance()/issubclass() with typing's unsubscripted aliases check
# against the class they stand for; subscripted aliases and the special
# forms refuse with CPython's messages.
import typing
from typing import (Any, AsyncIterator, Callable, ContextManager, Dict, FrozenSet, Hashable,
                    Iterable, Iterator, List, Literal, Mapping, MutableMapping, Optional,
                    Pattern, Sequence, Set, Sized, Tuple, Type, Union, ClassVar)
import collections, re


class Bag:
    def __iter__(self):
        return iter([1])

    def __len__(self):
        return 1


def show(label, f):
    try:
        r = f()
    except Exception as e:
        r = f'{type(e).__name__}: {e}'
    print(label, r)


cases = [
    ('list List', lambda: isinstance([1], List)),
    ('tuple List', lambda: isinstance((1,), List)),
    ('dict Dict', lambda: isinstance({}, Dict)),
    ('set Set', lambda: isinstance({1}, Set)),
    ('frozenset FrozenSet', lambda: isinstance(frozenset(), FrozenSet)),
    ('tuple Tuple', lambda: isinstance((), Tuple)),
    ('int Type', lambda: isinstance(int, Type)),
    ('str Iterable', lambda: isinstance('ab', Iterable)),
    ('Bag Iterable', lambda: isinstance(Bag(), Iterable)),
    ('Bag Sized', lambda: isinstance(Bag(), Sized)),
    ('int Iterable', lambda: isinstance(3, Iterable)),
    ('gen Iterator', lambda: isinstance((x for x in ()), Iterator)),
    ('list Iterator', lambda: isinstance([], Iterator)),
    ('dict Mapping', lambda: isinstance({}, Mapping)),
    ('OrderedDict MutableMapping', lambda: isinstance(collections.OrderedDict(), MutableMapping)),
    ('list Mapping', lambda: isinstance([], Mapping)),
    ('str Sequence', lambda: isinstance('x', Sequence)),
    ('set Sequence', lambda: isinstance({1}, Sequence)),
    ('len Callable', lambda: isinstance(len, Callable)),
    ('lambda Callable', lambda: isinstance(lambda: 0, Callable)),
    ('int Callable', lambda: isinstance(1, Callable)),
    ('int Hashable', lambda: isinstance(1, Hashable)),
    ('list Hashable', lambda: isinstance([], Hashable)),
    ('pattern Pattern', lambda: isinstance(re.compile('a'), Pattern)),
    ('tuple of aliases', lambda: isinstance({}, (List, Mapping))),
    ('mixed tuple', lambda: isinstance(5, (str, List, int))),
    ('typing.List attr', lambda: isinstance([], typing.List)),
    ('issubclass list Sequence', lambda: issubclass(list, Sequence)),
    ('issubclass dict Mapping', lambda: issubclass(dict, typing.Mapping)),
    ('issubclass int Iterable', lambda: issubclass(int, Iterable)),
    ('issubclass Bag Sized', lambda: issubclass(Bag, Sized)),
    ('issubclass List Sequence', lambda: issubclass(List, Sequence)),
    ('issubclass non-class', lambda: issubclass(1, List)),
    ('Optional[int]', lambda: isinstance(None, Optional[int])),
    ('Union', lambda: isinstance('a', Union[int, str])),
    ('List[int]', lambda: isinstance([], List[int])),
    ('Dict[str, int] sub', lambda: issubclass(dict, Dict[str, int])),
    ('Callable[[int], int]', lambda: isinstance(len, Callable[[int], int])),
    ('Any', lambda: isinstance(1, Any)),
    ('Optional', lambda: isinstance(1, Optional)),
    ('Literal', lambda: isinstance(1, Literal)),
    ('ClassVar sub', lambda: issubclass(int, ClassVar)),
]
for label, f in cases:
    show(label, f)

# The check runs where models write it: a type switch over JSON-like data.
def walk(value, depth=0):
    if isinstance(value, Mapping):
        return {k: walk(v, depth + 1) for k, v in value.items()}
    if isinstance(value, Sequence) and not isinstance(value, str):
        return [walk(v, depth + 1) for v in value]
    return (depth, value)


print(walk({'a': [1, {'b': 'c'}], 'd': (2,)}))

# Calling an alias: the inst=False ones refuse with CPython's message, the
# others build their origin.
for label, f in [
    ('List()', lambda: typing.List()),
    ('List[int]()', lambda: typing.List[int]()),
    ('FrozenSet()', lambda: typing.FrozenSet()),
    ('Tuple()', lambda: typing.Tuple()),
    ('Type()', lambda: typing.Type()),
    ('Dict[str, int](a=1)', lambda: typing.Dict[str, int](a=1)),
    ('Deque([1])', lambda: typing.Deque([1])),
    ('DefaultDict(int)', lambda: typing.DefaultDict(int)),
    ('Counter(aab)', lambda: typing.Counter('aab')),
    ('OrderedDict[str, int]()', lambda: typing.OrderedDict[str, int](b=2)),
    ('Optional()', lambda: typing.Optional()),
    ('Any()', lambda: typing.Any()),
    ('Mapping()', lambda: typing.Mapping()),
    ('Iterable[int]()', lambda: typing.Iterable[int]()),
]:
    show(label, f)
