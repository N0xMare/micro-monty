# Metaclasses other than type: __new__/__init__ with class keywords, __call__,
# metaclass methods and properties, inheritance of the metaclass, and
# calling the metaclass directly (docs/spec.md §4).
class Registry(type):
    classes = []

    def __new__(mcls, name, bases, ns, **kwargs):
        print('Registry.__new__', mcls.__name__, name, [b.__name__ for b in bases], sorted(k for k in ns if not k.startswith('__')), kwargs)
        cls = super().__new__(mcls, name, bases, ns)
        mcls.classes.append(name)
        return cls

    def __init__(cls, name, bases, ns, **kwargs):
        print('Registry.__init__', name, kwargs)
        super().__init__(name, bases, ns)

    def __call__(cls, *args, **kwargs):
        print('Registry.__call__', cls.__name__, args)
        return super().__call__(*args, **kwargs)

    def describe(cls):
        return f'<{cls.__name__} via {type(cls).__name__}>'

    @property
    def upper(cls):
        return cls.__name__.upper()


class Base(metaclass=Registry):
    def __init__(self, x):
        self.x = x


class Child(Base, tag='c'):
    y = 2


print(Registry.classes)
print(type(Base) is Registry, type(Child) is Registry, isinstance(Child, type))
print(Child.describe(), Child.upper)
c = Child(5)
print(c.x, c.y, type(c).__name__)
print(isinstance(c, Base), issubclass(Child, Base))
try:
    c.describe()
except AttributeError as e:
    print('AttributeError', e)


class Counted(type):
    def __call__(cls, *args):
        inst = super().__call__(*args)
        cls.count = getattr(cls, 'count', 0) + 1
        return inst


class P(metaclass=Counted):
    pass


P(); P()
print(P.count)


Dyn = Registry('Dyn', (Base,), {'z': 3})
print(Dyn.z, type(Dyn).__name__, Dyn(1).x)
