# super() corners and errors.
class Base:
    def __init__(self):
        self.trail = ['Base']

    def __repr__(self):
        return 'Base-repr'

    @property
    def kind(self):
        return 'base-kind'

    @classmethod
    def cm(cls):
        return f'Base.cm({cls.__name__})'

    @staticmethod
    def sm(x):
        return f'Base.sm({x})'


class Mid(Base):
    def __init__(self):
        super().__init__()
        self.trail.append('Mid')

    def __repr__(self):
        return 'Mid+' + super().__repr__()

    @property
    def kind(self):
        return 'mid/' + super().kind

    @classmethod
    def cm(cls):
        return 'Mid>' + super().cm()

    @staticmethod
    def sm(x):
        return 'Mid>' + super(Mid, Mid).sm(x)


class Leaf(Mid):
    def __init__(self):
        super(Mid, self).__init__()
        self.trail.append('Leaf')


print(Mid().trail, Leaf().trail, repr(Leaf()), Leaf().kind, Leaf.cm(), Leaf.sm(1))
print(super(Leaf, Leaf()).__repr__(), super(Mid, Leaf).cm())


class SetAttr:
    def __setattr__(self, k, v):
        super().__setattr__(k, str(v))


s = SetAttr()
s.x = 5
print(repr(s.x))


class E(Exception):
    def __init__(self, msg, code):
        super().__init__(msg)
        self.code = code

    def __str__(self):
        return f'{super().__str__()} (code {self.code})'


print(str(E('bad', 7)), E('bad', 7).args)


class NoArgs:
    def method(self):
        def inner():
            return super().method()
        return inner()


try:
    NoArgs().method()
except RuntimeError as e:
    print('RuntimeError', e)


class Stat:
    @staticmethod
    def f():
        return super().f()


try:
    Stat.f()
except RuntimeError as e:
    print('RuntimeError', e)
try:
    super(Base, Mid)
    super(Mid, Base())
except TypeError as e:
    print('TypeError', e)
try:
    super(1, 2)
except TypeError as e:
    print('TypeError', e)


class Multi(Base):
    def describe(self):
        return super().nonexistent


try:
    Multi().describe()
except AttributeError as e:
    print('AttributeError', e)


class A:
    def go(self):
        return ['A']


class B(A):
    def go(self):
        return ['B'] + super().go()


class C(A):
    def go(self):
        return ['C'] + super().go()


class D(B, C):
    def go(self):
        return ['D'] + super().go()


print(D().go(), super(B, D()).go(), super(C, D()).go())


class Shadow(A):
    def go(self):
        __class__ = None
        return 'shadowed'


print(Shadow().go())
print(type(super(D, D())).__name__, repr(super(D, D())).startswith('<super:'))
