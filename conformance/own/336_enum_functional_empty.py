# The functional Enum API without names: a data type alone creates an empty
# enum; nothing at all is a TypeError; a class with members does lookups.
from enum import Enum, IntEnum, Flag

E = Enum("E", type=int)
print(E, len(E), list(E), issubclass(E, int))
F = Enum("F", names=())
print(F, len(F))
G = Enum("G", None)
print(G, len(G))
for call in (lambda: Enum("H"), lambda: IntEnum("I"), lambda: Flag("J")):
    try:
        call()
    except TypeError as e:
        print("TypeError:", e)

Color = Enum("Color", "RED GREEN")
print(Color(1), Color(2), list(Color))
try:
    Color("Shade", "DARK LIGHT")
except ValueError as e:
    print("ValueError:", e)


class Pair(Enum):
    A = (1, 2)
    B = (3, 4, 5)


print(Pair(1, 2), Pair(3, 4, 5), Pair((1, 2)))
try:
    Pair(1)
except ValueError as e:
    print("ValueError:", e)
