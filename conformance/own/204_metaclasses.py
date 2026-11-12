# Metaclasses: __new__/__init__/__call__ on the metaclass, class keywords,
# metaclass dunders on classes (len, iter, getitem, contains, repr),
# metaclass methods and properties, type(name, bases, ns), inheritance of
# the metaclass, __instancecheck__/__subclasscheck__ (also in match).
class Meta(type):
    def __new__(mcls, name, bases, ns, **kw):
        ns['added'] = name.lower()
        cls = super().__new__(mcls, name, bases, ns)
        cls.kw = kw
        return cls

    def __init__(cls, name, bases, ns, **kw):
        super().__init__(name, bases, ns)
        cls.inited = True

    def __call__(cls, *args, **kw):
        obj = super().__call__(*args, **kw)
        obj.via_meta = True
        return obj

    def __len__(cls):
        return 42

    def __iter__(cls):
        return iter([1, 2, 3])

    def __getitem__(cls, k):
        return ('item', k)

    def __contains__(cls, x):
        return x == 'yes'

    def __repr__(cls):
        return '<Meta ' + cls.__name__ + '>'

    def hello(cls):
        return 'hello from ' + cls.__name__

    @property
    def prop(cls):
        return 'prop of ' + cls.__name__


class Base(metaclass=Meta, flag=1):
    def __init__(self, x=0):
        self.x = x


class Child(Base):
    def method(self):
        return __class__.__name__


print(type(Base).__name__, type(Child) is Meta, Base.added, Child.added, Base.kw, Child.kw, Base.inited)
b = Child(5)
print(b.x, b.via_meta, b.method(), isinstance(Child, Meta), isinstance(b, Base), Base.__class__)
print(len(Base), list(Child), Base['k'], 'yes' in Base, 'no' in Child, repr(Child))
print(Child.hello(), Child.prop, Base.__name__)

class Plain:
    pass

C2 = Meta('Dyn', (Plain,), {'z': 1})
print(type(C2).__name__, C2.z, C2.added, C2.__mro__[1].__name__)
T3 = type('T3', (), {'a': 1})
print(T3.a, type(T3).__name__)


class Check(type):
    def __instancecheck__(cls, inst):
        return isinstance(inst, int)

    def __subclasscheck__(cls, sub):
        return sub is bool


class Intish(metaclass=Check):
    pass


print(isinstance(3, Intish), isinstance('x', Intish), issubclass(bool, Intish), issubclass(int, Intish), isinstance(3, (str, Intish)))


class OnlyInit(type):
    def __init__(cls, name, bases, ns):
        super().__init__(name, bases, ns)
        cls.registry = []


class R(metaclass=OnlyInit):
    pass


print(R.registry, type(R).__name__)
match 5:
    case Intish():
        print('matched Intish')
