# type.__new__ fills the class body's `__class__` cell (`__classcell__` in
# the namespace a metaclass sees), so `super()` and `__class__` work in
# methods the metaclass calls before it returns (enum members made by a
# `__new__` that uses super()); an enum's `__new__` is the value lookup, so
# a member `__new__` calling `super().__new__` gets CPython's TypeError.
import enum
class Meta(type):
    def __new__(mcls, name, bases, ns, **kw):
        print('classcell' if '__classcell__' in ns else 'no cell')
        cls = super().__new__(mcls, name, bases, ns)
        if hasattr(cls, 'make'):
            print(cls.make())
        return cls
class A(metaclass=Meta):
    @classmethod
    def make(cls):
        return super().__repr__ is not None, __class__.__name__
class B(A):
    def g(self): return 1
class Big(bytes, enum.Enum):
    def __new__(cls, v):
        obj = bytes.__new__(cls, [v])
        obj._value_ = v
        return obj
    X = 1
print(Big.X, Big.X.value)
class Dup(int, enum.Enum):
    def __new__(cls, v):
        obj = super().__new__(cls, v)
        return obj
    A = 1
print("made?", Dup.A)
import enum
for base in (enum.Enum, enum.IntEnum, enum.Flag):
    try:
        class BadSuper(base):
            def __new__(cls, value):
                obj = super().__new__(cls, value)
                return obj
            failed = 1
        print("no error", list(BadSuper))
    except TypeError as e: print(e)
