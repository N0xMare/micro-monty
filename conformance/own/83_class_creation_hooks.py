# Class decorators, __init_subclass__, __set_name__, type(name, bases, dict).
registry = []


def register(cls):
    registry.append(cls.__name__)
    cls.registered = True
    return cls


def tag(label):
    def deco(cls):
        cls.label = label
        return cls
    return deco


@register
@tag('t1')
class First:
    pass


print(registry, First.registered, First.label)


class Plugin:
    plugins = []

    def __init_subclass__(cls, kind='default', **kw):
        super().__init_subclass__(**kw)
        cls.kind = kind
        Plugin.plugins.append((cls.__name__, kind))


class A(Plugin):
    pass


class B(Plugin, kind='special'):
    pass


class C(B):
    pass


print(Plugin.plugins, C.kind, hasattr(Plugin, 'kind'))
try:
    class D(Plugin, unknown=1):
        pass
except TypeError as e:
    print(e)


class Field:
    def __set_name__(self, owner, name):
        print('set_name', owner.__name__, name)
        self.name = '_' + name

    def __get__(self, obj, objtype=None):
        if obj is None:
            return self
        return getattr(obj, self.name, 'unset')

    def __set__(self, obj, value):
        setattr(obj, self.name, value)


class Model:
    x = Field()
    y = Field()


m = Model()
m.x = 5
print(m.x, m.y, type(Model.x).__name__, sorted(vars(m)))


def method(self):
    return f'dynamic {self.v}'


Dyn = type('Dyn', (object,), {'v': 7, 'show': method, '__repr__': lambda s: 'Dyn()'})
print(Dyn().show(), Dyn(), Dyn.__name__, Dyn.__qualname__, Dyn.__module__, Dyn.__bases__)
Sub = type('Sub', (Dyn,), {})
print(Sub().show(), isinstance(Sub(), Dyn), type(Sub()).__mro__)
E = type('E', (ValueError,), {'code': 3})
try:
    raise E('typed')
except ValueError as e:
    print(type(e).__name__, e, e.code)
print(type(type), type(int), type(Dyn))


class WithClassGetItem:
    def __class_getitem__(cls, item):
        return f'{cls.__name__}[{item}]'


print(WithClassGetItem[int], WithClassGetItem["x"])


class Meta1:
    def __init_subclass__(cls):
        cls.order = getattr(cls, 'order', 0) + 1


class L1(Meta1):
    pass


class L2(L1):
    pass


print(L1.order, L2.order)


class Ns:
    a = 1
    b = a + 1
    c = [a * i for i in range(3)]

    def m(self):
        return Ns.a


print(Ns.b, Ns.c, Ns().m())
x = 'global'


class Scope:
    x = 'class'
    y = x

    def get(self):
        return x


print(Scope.y, Scope().get())


class Docs:
    """Docstring here."""

    def method(self):
        """Method doc."""


print(Docs.__doc__, Docs.method.__doc__, Docs().method.__doc__)


class Counter:
    count = 0

    def __new__(cls, *args):
        cls.count += 1
        inst = super().__new__(cls)
        inst.created = cls.count
        return inst

    def __init__(self, v):
        self.v = v


c1, c2 = Counter(1), Counter(2)
print(c1.created, c2.created, c2.v, Counter.count)


class Singleton:
    _inst = None

    def __new__(cls):
        if cls._inst is None:
            cls._inst = super().__new__(cls)
        return cls._inst


print(Singleton() is Singleton())


class NotInst:
    def __new__(cls):
        return 42

    def __init__(self):
        print('never runs')


print(NotInst())
