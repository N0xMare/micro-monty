# __new__ corners: arguments, returning other types, subclasses, object.__new__.
class Base:
    def __new__(cls, *args, **kwargs):
        print('Base.__new__', cls.__name__, args, kwargs)
        return super().__new__(cls)

    def __init__(self, *args, **kwargs):
        print('Base.__init__', args, kwargs)
        self.args = args


b = Base(1, k=2)
print(b.args)


class Sub(Base):
    def __init__(self, x):
        print('Sub.__init__', x)
        super().__init__(x, x)


s = Sub(5)
print(type(s).__name__, s.args)


class Factory:
    def __new__(cls, kind):
        if kind == 'sub':
            return super().__new__(FactorySub)
        if kind == 'int':
            return 7
        return super().__new__(cls)

    def __init__(self, kind):
        print('Factory.__init__', type(self).__name__, kind)


class FactorySub(Factory):
    pass


print(type(Factory('base')).__name__, type(Factory('sub')).__name__, Factory('int'))


class Other:
    def __init__(self):
        print('never')


class ReturnsOther:
    def __new__(cls):
        return Other.__new__(Other)

    def __init__(self):
        print('ReturnsOther.__init__ never')


print(type(ReturnsOther()).__name__)


class NoNewArgs:
    pass


try:
    object.__new__(NoNewArgs, 1)
except TypeError as e:
    print(e)
print(type(object.__new__(NoNewArgs)).__name__)
try:
    object.__new__(3)
except TypeError as e:
    print(e)


class InitOnly:
    def __init__(self, a):
        self.a = a


print(InitOnly(3).a)
try:
    InitOnly()
except TypeError as e:
    print(e)


class NewOnly:
    def __new__(cls, a):
        inst = super().__new__(cls)
        inst.a = a
        return inst


print(NewOnly(4).a)
try:
    NewOnly(1, 2)
except TypeError as e:
    print(e)


class Cached:
    _cache = {}

    def __new__(cls, key):
        if key not in cls._cache:
            cls._cache[key] = super().__new__(cls)
        return cls._cache[key]

    def __init__(self, key):
        self.inits = getattr(self, 'inits', 0) + 1


a, b2, c = Cached('x'), Cached('x'), Cached('y')
print(a is b2, a is c, a.inits, c.inits)


class ExcNew(Exception):
    def __new__(cls, *args):
        inst = super().__new__(cls, *args)
        inst.tag = 'tagged'
        return inst


try:
    raise ExcNew('m')
except ExcNew as e:
    print(e.tag, e.args, str(e))
