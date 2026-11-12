# str subclasses: `str.__str__` gives the text (not the subclass's repr),
# `__repr__`/`__str__` may return a str subclass, getattr/setattr/hasattr/
# delattr take a str subclass name (a StrEnum member). Enum formatting as
# CPython's: IntEnum/StrEnum/IntFlag format with an empty spec as str(self),
# so an overridden __str__ shows; a direct ReprEnum mixin takes the data
# type's __str__/__format__ (float, date).
class S(str):
    def __repr__(self): return "S!"
s = S("ab")
print(str.__str__(s), type(str.__str__(s)).__name__, str(s), type(str(s)).__name__, repr(s), f"{s}", "%s" % s, format(s), str.__repr__(s))
class R:
    def __repr__(self): return S("r")
    def __str__(self): return S("s")
print(repr(R()), str(R()), repr(R()) == "r", str(R()) + "!")
print(getattr(R(), S("__class__")).__name__)
class I(int):
    def __repr__(self): return "I!"
    def __str__(self): return "Istr"
print(int.__repr__(I(3)), int.__str__(I(3)), str(I(3)), f"{I(3)}", format(I(3), "d"), "%d" % I(3))
class F(float):
    def __repr__(self): return "F!"
print(float.__repr__(F(1.5)), str(F(1.5)), f"{F(1.5)}", f"{F(1.5):.2f}")
import enum
class NS(enum.IntEnum):
    A = 1
    def __str__(self): return self.name.upper()
print(format(NS.A), f"{NS.A}", str(NS.A))
class FE(float, enum.ReprEnum):
    X = 1.5
print(format(FE.X), str(FE.X), f"{FE.X:.2f}")
class SE(enum.StrEnum):
    A = enum.auto()
    def __str__(self): return "zz"
print(format(SE.A), f"{SE.A}")
class Color(enum.Enum):
    RED = "r"
    GREEN = "g"
print(f"{Color.RED}", format(Color.RED, ">10"), Color("r"), Color["GREEN"])
class P(enum.Flag):
    R = enum.auto(); W = enum.auto()
print(P.R | P.W, list(P.R | P.W), repr(P(0)))
import datetime
class DE(datetime.date, enum.ReprEnum):
    D = (2009, 1, 1)
print(format(DE.D), str(DE.D), f"{DE.D:%Y}", repr(DE.D))
class SE2(enum.StrEnum):
    A = "a"
    def __str__(self): return "custom"
print(f"{SE2.A}", f"{SE2.A:>5}", "{}".format(SE2.A))
class IF(enum.IntFlag):
    X = 1; Y = 2
    def __str__(self): return "flag!"
print(f"{IF.X}", f"{IF.X | IF.Y}", format(IF.Y, "03d"))
try:
    class Bad(enum.ReprEnum):
        A = 1
except TypeError as e: print(e)
class Field(enum.StrEnum):
    NAME = "name"
class Obj: pass
o = Obj()
setattr(o, Field.NAME, 5)
print(getattr(o, Field.NAME), hasattr(o, Field.NAME), o.name)
delattr(o, Field.NAME)
print(hasattr(o, "name"))
import enum
class F(float, enum.ReprEnum):
    pass
class G(F):
    def __str__(self): return "G!"
    B = 1.5
class I(int):
    def __repr__(self): return "I!"
class J(int):
    def __str__(self): return "J!"
class SR(str):
    def __repr__(self): return "SR!"
print(format(G.B), f"{G.B}", f"{G.B:.2f}", format(I(3)), format(J(3)), format(J(3), "d"), format(SR("t")), float.__format__(G.B, ""), int.__format__(J(2), ""))
