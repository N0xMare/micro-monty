# A dataclass without a doc-string gets its signature as one, as CPython
# makes it: the class name and the fields `__init__` takes, with their
# annotations as written and their defaults.
import dataclasses
import typing


@dataclasses.dataclass
class P:
    x: int
    y: str = 'a'
print(repr(P.__doc__))
@dataclasses.dataclass
class Q:
    """doc"""
    x: int
print(repr(Q.__doc__))
@dataclasses.dataclass
class R:
    pass
print(repr(R.__doc__))
@dataclasses.dataclass
class S:
    x: int
    y: list = dataclasses.field(default_factory=list)
    z: 'S | None' = None
    w: dict[str, int] = dataclasses.field(default_factory=dict, kw_only=True)
    _: dataclasses.KW_ONLY
    k: int = 3
    i: dataclasses.InitVar[int] = 0
    def __post_init__(self, i): pass
print(repr(S.__doc__))
@dataclasses.dataclass(init=False)
class T:
    x: int
print(repr(T.__doc__))
@dataclasses.dataclass
class U(P):
    z: float = 1.0
print(repr(U.__doc__))
@dataclasses.dataclass
class V:
    c: typing.ClassVar[int] = 0
    f: int = dataclasses.field(default=5, init=False)
    p: 'int' = 1
print(repr(V.__doc__))
M = dataclasses.make_dataclass('M', [('a', int), ('b', str, 'q')])
print(repr(M.__doc__))


@dataclasses.dataclass(init=False)
class OwnInit:
    x: int

    def __init__(self, x, y=2):
        self.x = x


# micro-monty has no `inspect` to read a hand-written `__init__`: no
# doc-string is made for it (CPython: 'OwnInit(x, y=2)').
print(OwnInit.__doc__ in (None, 'OwnInit(x, y=2)'))


@dataclasses.dataclass(frozen=True, order=True)
class Fz:
    a: int = 0
    b: tuple = ()
    c: str = dataclasses.field(default='s', repr=False)
    d: float = dataclasses.field(default=1.5, kw_only=True)


print(repr(Fz.__doc__))


@dataclasses.dataclass
class Empty:
    """"""
    x: int = 1


print(repr(Empty.__doc__))
print(repr(P(1).__doc__))


# A default whose repr raises ValueError or TypeError: the class is still
# made, and its doc-string is its name.
class BadValue:
    def __repr__(self):
        raise ValueError('bad repr')


class BadType:
    def __repr__(self):
        raise TypeError('bad repr')


@dataclasses.dataclass
class WithBadValue:
    x: int = 1
    y: object = BadValue()


@dataclasses.dataclass
class WithBadType:
    y: object = BadType()


print(repr(WithBadValue.__doc__), repr(WithBadType.__doc__), WithBadValue().x)


# Another exception from a repr leaves the decorator, as on CPython.
class BadOther:
    def __repr__(self):
        raise KeyError('bad repr')


try:
    @dataclasses.dataclass
    class WithBadOther:
        y: object = BadOther()
except KeyError as e:
    print('KeyError', e)
