# dataclasses beyond the corpus: order, kw_only, InitVar, init=False,
# inheritance, the helpers, and make_dataclass.
from dataclasses import (KW_ONLY, InitVar, asdict, astuple, dataclass, field,
                         fields, make_dataclass, replace, FrozenInstanceError)


@dataclass(order=True)
class Version:
    major: int
    minor: int = 0
    tag: str = field(default='', compare=False)


print(sorted([Version(1, 2), Version(1, 0, 'x'), Version(0, 9)]))
print(Version(1) < Version(1, 1), Version(1, 0, 'a') == Version(1, 0, 'b'))


@dataclass
class Opts:
    name: str
    _: KW_ONLY
    verbose: bool = False
    level: int = 1


print(Opts('a', level=3), [f.kw_only for f in fields(Opts)])
try:
    Opts('a', True)
except TypeError as e:
    print('TypeError', e)


@dataclass
class Scaled:
    raw: InitVar[int]
    factor: int = 2
    value: int = field(init=False)

    def __post_init__(self, raw):
        self.value = raw * self.factor


s = Scaled(5)
print(s, s.value, [f.name for f in fields(s)])


@dataclass
class Base:
    x: int
    tags: list = field(default_factory=list)


@dataclass
class Child(Base):
    y: int = 0


c = Child(1, ['t'], 2)
print(c, asdict(c), astuple(c), replace(c, y=9))
print(asdict(Child(1, [Base(2)])))


@dataclass(frozen=True, unsafe_hash=False)
class Key:
    a: int
    b: tuple = ()


k = Key(1, (2,))
print(hash(k) == hash((1, (2,))), {k: 1}[Key(1, (2,))])
try:
    k.a = 3
except FrozenInstanceError as e:
    print('FrozenInstanceError', e)
try:
    del k.a
except FrozenInstanceError as e:
    print('FrozenInstanceError', e)

Made = make_dataclass('Made', [('a', int), ('b', int, field(default=4))], order=True)
print(Made(1), Made(1) < Made(2), Made.__name__)


@dataclass(eq=False, repr=False)
class Plain:
    v: int


print(Plain(1) == Plain(1), hasattr(Plain, '__match_args__'), Plain.__match_args__)
try:
    @dataclass(order=True, eq=False)
    class Wrong:
        v: int
except ValueError as e:
    print('ValueError', e)
try:
    @dataclass(order=True)
    class Clash:
        v: int

        def __lt__(self, other):
            return True
except TypeError as e:
    print('TypeError', e)
try:
    replace(s, value=3)
except (TypeError, ValueError) as e:
    print(type(e).__name__, e)
