# property getter/setter/deleter, staticmethod, classmethod on subclasses.
class Temp:
    def __init__(self, c):
        self._c = c

    @property
    def c(self):
        """Celsius."""
        return self._c

    @c.setter
    def c(self, v):
        if v < -273:
            raise ValueError('too cold')
        self._c = v

    @c.deleter
    def c(self):
        print('deleting')
        del self._c

    @property
    def f(self):
        return self._c * 9 / 5 + 32


t = Temp(100)
print(t.c, t.f)
t.c = 0
print(t.c, t.f)
try:
    t.c = -300
except ValueError as e:
    print('ValueError', e)
try:
    t.f = 1
except AttributeError as e:
    print('AttributeError', e)
del t.c
try:
    t.c
except AttributeError as e:
    print('AttributeError', e)
print(Temp.c.__doc__, type(Temp.c).__name__, Temp.c.fget.__name__)
try:
    del t.f
except AttributeError as e:
    print('AttributeError', e)


class Shape:
    count = 0

    @classmethod
    def create(cls, *args):
        cls.count += 1
        return cls(*args)

    @staticmethod
    def unit():
        return 1

    def __init__(self, *args):
        self.args = args


class Square(Shape):
    pass


sq = Square.create(3)
print(type(sq).__name__, sq.args, Square.count, Shape.count)
print(Shape.create().args, Shape.count, Square.count)
print(sq.unit(), Square.unit(), sq.create(4).args)
cm = Shape.__dict__['create']
sm = Shape.__dict__['unit']
print(type(cm).__name__, type(sm).__name__, sm.__func__(), cm.__func__(Square).args)


def get_x(self):
    return self._x * 2


def set_x(self, v):
    self._x = v


class Manual:
    x = property(get_x, set_x, None, 'the x')


m = Manual()
m.x = 21
print(m.x, Manual.x.__doc__)
m.__dict__['x'] = 'shadow'
print(m.x)


class Ro:
    @property
    def val(self):
        return 'ro'


r = Ro()
try:
    r.val = 2
except AttributeError as e:
    print(e)


class Child(Temp):
    @property
    def c(self):
        return 'child:' + str(super().c)


print(Child(5).c)
