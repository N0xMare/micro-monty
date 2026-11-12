# enum's bit helpers: _iter_bits_lsb and show_flag_values yield the set
# bits lowest first and reject negative values (they used to loop forever),
# with an Enum member's value accepted, as CPython 3.14's enum does.
import enum
from enum import _iter_bits_lsb, show_flag_values, Flag, IntFlag, auto


class Color(Flag):
    RED = auto()
    GREEN = auto()
    BLUE = auto()


class Perm(IntFlag):
    R = 4
    W = 2
    X = 1


print(list(_iter_bits_lsb(0)), list(_iter_bits_lsb(1)), list(_iter_bits_lsb(0b101101)))
print(list(_iter_bits_lsb(1 << 100 | 3)))
print(show_flag_values(6), show_flag_values(Perm.R | Perm.X))
print(list(_iter_bits_lsb(Color.BLUE)), list(_iter_bits_lsb(Perm.W)))
for bad in (-8, -1, Perm(0) - 3):
    try:
        list(_iter_bits_lsb(bad))
    except ValueError as e:
        print("ValueError", e)
try:
    show_flag_values(-5)
except ValueError as e:
    print("ValueError", e)
print(list(Color.RED | Color.BLUE), list(Perm(7)), [m.name for m in Perm.R | Perm.W])
