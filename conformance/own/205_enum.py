# enum: Enum/IntEnum/StrEnum/Flag/IntFlag, auto (also combined in Flag
# bodies), aliases, unique, member/nonmember, functional API, _missing_,
# metaclass __setattr__/__delattr__ guards, repr/str/format, match.
import enum
from enum import Enum, IntEnum, StrEnum, Flag, IntFlag, auto, unique, member, nonmember


class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3
    CRIMSON = 1

    def describe(self):
        return f'{self.name} is {self.value}'

    @classmethod
    def default(cls):
        return cls.GREEN


print(Color.RED, repr(Color.RED), Color.CRIMSON is Color.RED, Color(2), Color['BLUE'])
print(list(Color), len(Color), Color.RED in Color, 2 in Color, list(Color.__members__))
print(Color.RED.describe(), Color.default(), type(Color.RED).__name__, isinstance(Color.RED, Color))
print(repr(Color), Color.RED.name, Color.RED.value, hash(Color.RED) == hash('RED'))
print({Color.RED: 'r'}[Color.CRIMSON], Color.RED == Color.RED, Color.RED != Color.GREEN, Color.RED == 1)
print(f'{Color.RED}', format(Color.GREEN, '>12'), str(Color.BLUE))
try:
    Color(7)
except ValueError as e:
    print('ValueError', e)
try:
    Color['PURPLE']
except KeyError as e:
    print('KeyError', e)


class Num(IntEnum):
    ONE = 1
    TWO = 2


print(Num.ONE + 1, Num.TWO * 3, Num.ONE < Num.TWO, Num(2), repr(Num.ONE), str(Num.ONE), f'{Num.TWO:03d}', Num.ONE == 1, int(Num.TWO))
print(sorted([Num.TWO, Num.ONE]), {1: 'one'}[Num.ONE], isinstance(Num.ONE, int))


class Shade(StrEnum):
    DARK = auto()
    LIGHT = 'light-ish'


print(Shade.DARK, repr(Shade.DARK), Shade.DARK.upper(), Shade('light-ish'), Shade.DARK == 'dark', f'{Shade.LIGHT}')


class Auto(Enum):
    A = auto()
    B = auto()
    C = 10
    D = auto()
    _private = 5
    helper = nonmember(3)
    fn = member(len)


print([(m.name, m.value) for m in Auto], Auto.helper, Auto.fn.value is len)


class Perm(Flag):
    R = 4
    W = 2
    X = 1
    RWX = 7


print(Perm.R | Perm.W, repr(Perm.R | Perm.W), Perm.RWX, list(Perm.R | Perm.X), Perm.R in Perm.RWX, bool(Perm(0)))
print(~Perm.R, Perm(6), len(Perm.RWX), Perm.W & Perm.RWX, Perm.R ^ Perm.RWX, repr(Perm(0)))
try:
    Perm(8)
except ValueError as e:
    print('ValueError', e)


class Bits(IntFlag):
    A = auto()
    B = auto()
    C = auto()


print(Bits.A | Bits.C, repr(Bits.A | 8), Bits.B | 1, int(Bits.C), ~Bits.A, list(Bits), Bits.A + 1)

Animal = Enum('Animal', 'ANT BEE CAT')
print(list(Animal), Animal.BEE.value, repr(Animal))
Pet = Enum('Pet', [('DOG', 'd'), ('FISH', 'f')])
print(Pet('d'), Pet.FISH.value)

try:
    @unique
    class Dup(Enum):
        A = 1
        B = 1
except ValueError as e:
    print('ValueError', e)


class Planet(Enum):
    MERCURY = (3.303e+23, 2.4397e6)
    EARTH = (5.976e+24, 6.37814e6)

    def __init__(self, mass, radius):
        self.mass = mass
        self.radius = radius


print(Planet.EARTH.mass, Planet.MERCURY.value, Planet((5.976e+24, 6.37814e6)))


class Missing(Enum):
    A = 'a'

    @classmethod
    def _missing_(cls, value):
        if isinstance(value, str) and value.lower() == 'a':
            return cls.A
        return None


print(Missing('A'), [c.name for c in reversed(Color)])
match Color.GREEN:
    case Color.RED:
        print('red')
    case Color.GREEN:
        print('green!')


class Color(Enum):
    RED = 1
    GREEN = 2
    ALIAS = 1


print(Color.__members__)
print(Color.RED.__class__, Color.RED._name_, Color.RED._value_)
try:
    Color.RED < Color.GREEN
except TypeError as e:
    print('TypeError', e)
try:
    Color.RED = 5
except AttributeError as e:
    print('AttributeError', e)
try:
    del Color.RED
except AttributeError as e:
    print('AttributeError', e)
try:
    class Bad(Color):
        X = 3
except TypeError as e:
    print('TypeError', e)
print(len(set(Color)), sorted(Color, key=lambda m: m.value))
print(isinstance(Color, enum.EnumType), type(Color), issubclass(Color, Enum), Color.__name__)


class Weekday(IntEnum):
    MON = 1
    TUE = 2


print(Weekday.MON + Weekday.TUE, Weekday(1) is Weekday.MON, max(Weekday), [d.name for d in Weekday], Weekday.TUE.value)
print('%s %d %r' % (Weekday.MON, Weekday.TUE, Weekday.MON))


class Open(IntFlag):
    R = 4
    W = 2
    X = 1


print(Open.R | Open.W, Open(7), Open(0), ~Open.R, Open.R | 16, repr(Open(-1)), Open.R & 4, 4 | Open.W)


class Color2(Flag):
    RED = auto()
    GREEN = auto()
    BLUE = auto()
    WHITE = RED | GREEN | BLUE


print(Color2.WHITE, list(Color2), list(Color2.WHITE), Color2.RED | Color2.GREEN, Color2(3).name, Color2(0).name)
print(Color2.RED in Color2.WHITE, Color2.WHITE.value, ~Color2.WHITE, ~Color2.RED)
try:
    1 in Color2.WHITE
except TypeError as e:
    print('TypeError', e)


class Mood(Enum):
    HAPPY = 'h'
    SAD = 's'

    def __str__(self):
        return 'mood:' + self.name

    @property
    def emoji(self):
        return ':)' if self is Mood.HAPPY else ':('


print(Mood.HAPPY, f'{Mood.SAD}', Mood.SAD.emoji, repr(Mood.HAPPY))


class Status(StrEnum):
    OK = 'ok'
    ERR = auto()


print(Status.OK + '!', Status.ERR, '{}'.format(Status.OK), Status('ok'), len(Status.OK), Status.OK.value)
try:
    Status(5)
except ValueError as e:
    print('ValueError', e)
for m in Color:
    match m:
        case Color.RED:
            print('r')
        case _:
            print('other', m)
