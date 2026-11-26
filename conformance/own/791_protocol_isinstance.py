# `isinstance` against a `@runtime_checkable` protocol is structural: an
# object whose type has every member the protocol and its protocol bases
# define or annotate is an instance, whatever its class; a subclass is one
# by inheritance. A protocol without `@runtime_checkable` refuses the check.
# `get_protocol_members` gives the same members, as a frozenset.
import typing
from typing import Protocol, runtime_checkable


@runtime_checkable
class Sized(Protocol):
    def __len__(self): ...


@runtime_checkable
class Named(Protocol):
    name: str

    def greet(self): ...


@runtime_checkable
class Loud(Named, Protocol):
    def shout(self): ...


class Plain(Protocol):
    def f(self): ...


class G:
    name = 'g'

    def greet(self):
        return 1


class GL(G):
    def shout(self):
        return 2


class OnlyName:
    def __init__(self):
        self.name = 'n'


class Impl(Sized):
    def __len__(self):
        return 0


assert isinstance([], Sized) and isinstance({}, Sized) and isinstance('', Sized)
assert not isinstance(3, Sized) and not isinstance(None, Sized)
assert isinstance(G(), Named) and not isinstance(object(), Named) and not isinstance(OnlyName(), Named)
assert isinstance(GL(), Loud) and not isinstance(G(), Loud) and isinstance(GL(), Named)
assert isinstance(Impl(), Sized) and issubclass(Impl, Sized) and isinstance(Impl(), Impl)
assert typing.get_protocol_members(Named) == frozenset({'name', 'greet'})
assert typing.get_protocol_members(Loud) == frozenset({'name', 'greet', 'shout'})
assert type(typing.get_protocol_members(Sized)) is frozenset
assert typing.is_protocol(Loud) and not typing.is_protocol(Impl) and not typing.is_protocol(G)
try:
    isinstance(1, Plain)
except TypeError as e:
    assert str(e) == 'Instance and class checks can only be used with @runtime_checkable protocols'
else:
    raise AssertionError('a protocol without @runtime_checkable took isinstance')
print('ok')
