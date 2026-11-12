# Single and multiple inheritance, C3 MRO, diamonds and MRO errors.
class A:
    def who(self):
        return 'A'

    def chain(self):
        return ['A']


class B(A):
    def who(self):
        return 'B'

    def chain(self):
        return ['B'] + super().chain()


class C(A):
    def who(self):
        return 'C'

    def chain(self):
        return ['C'] + super().chain()


class D(B, C):
    def chain(self):
        return ['D'] + super().chain()


print([k.__name__ for k in D.__mro__])
print(D().who(), D().chain())
print(issubclass(D, A), issubclass(A, D), issubclass(D, (int, C)))
print(isinstance(D(), B), isinstance(D(), (int, str)), isinstance(3, object))
print(D.__bases__, D.__base__)


class X: pass
class Y: pass
class Z(X, Y): pass
class W(Y, X): pass


for bases in [(Z, W), (X, Z), (A, A)]:
    try:
        type('Bad', bases, {})
    except TypeError as e:
        print('TypeError:', e)

try:
    class Bad(X, Z):
        pass
except TypeError as e:
    print('TypeError:', e)


class K1: pass
class K2: pass
class K3: pass
class K4(K1, K2): pass
class K5(K2, K3): pass
class K6(K4, K5): pass
print([k.__name__ for k in K6.mro()])


class Base:
    greeting = 'hi'

    def hello(self):
        return self.greeting + ' from ' + type(self).__name__


class Derived(Base):
    greeting = 'yo'


print(Derived().hello(), Base().hello())
Base.greeting = 'hey'
Derived.greeting = 'sup'
del Derived.greeting
print(Derived().hello())
Base.hello = lambda self: 'patched'
print(Derived().hello())
try:
    issubclass(3, int)
except TypeError as e:
    print(e)
try:
    isinstance(3, 4)
except TypeError as e:
    print(e)
