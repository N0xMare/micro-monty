# `obj.__class__ = C`: micro-monty never changes an object's class. Where
# CPython raises a TypeError, it raises the same one; assigning the class
# an object already has changes nothing; a `__class__` property or a
# `__setattr__` runs first, as in CPython.
import math


def t(label, f):
    try:
        f()
        print(label, 'ok')
    except Exception as e:
        print(label, type(e).__name__, e)


class A:
    pass


class C(A):
    pass


class I(int):
    pass


class L(list):
    pass


class E(Exception):
    pass


class Meta(type):
    pass


class WM(metaclass=Meta):
    pass


def setc(o, c):
    def g():
        o.__class__ = c

    return g


a = A()
t('same class', setc(a, A))
print(type(a).__name__)
t('to int', setc(A(), int))
t('to an int subclass', setc(A(), I))
t('int subclass to int', setc(I(1), int))
t('int to a subclass', setc(1, I))
t('list subclass to list', setc(L(), list))
t('to a list subclass', setc(A(), L))
t('exception to a class', setc(E(), A))
t('builtin exception', setc(Exception(), E))
t('to 3', setc(A(), 3))
t('to None', setc(A(), None))
t('object()', setc(object(), A))
t('to object', setc(A(), object))
t('function', setc(lambda: 0, A))
t('module', setc(math, A))
t('class to a metaclass', setc(A, Meta))
t('to type', setc(WM, type))
t('to Exception', setc(A(), Exception))
t('setattr', lambda: setattr(A(), '__class__', 4))
t('object.__setattr__', lambda: object.__setattr__(A(), '__class__', int))


def d(o):
    def g():
        del o.__class__

    return g


t('del', d(A()))
t('del int', d(1))
t('del class', d(A))


class P:
    @property
    def __class__(self):
        return A

    @__class__.setter
    def __class__(self, v):
        print('setter', v.__name__)


P().__class__ = C


class SA:
    def __setattr__(self, k, v):
        print('__setattr__', k)


SA().__class__ = A
