# dir(): scopes, instances, classes, modules, builtin types and __dir__.
import asyncio
import math


def public(names):
    return [n for n in names if not n.startswith('__')]


# Builtin types list their methods and dunders.
print('append' in dir([]), '__len__' in dir(list), '__init__' in dir(object))
print('keys' in dir({}), 'upper' in dir(''), 'add' in dir(set()), 'count' in dir(()))
print('bit_length' in dir(1), 'real' in dir(1), '__add__' in dir(int), '__bool__' in dir(0.5))
print('args' in dir(ValueError()), 'with_traceback' in dir(Exception), 'errno' in dir(OSError))
print('__dir__' in dir(object), '__mro__' in dir(type), '__class__' in dir(None))
print(dir(1) == dir(int), dir([]) == sorted(set(dir([]))), dir(True) == dir(bool))
print(set(dir(object)) <= set(dir(int)), set(dir(int)) <= set(dir(bool)))
print('__dict__' in dir(type), '__dict__' in dir(int), 'bit_length' in int.__dict__)

# Module level: the globals.
x = 1
print(public(dir()))


# Function scope: bound locals, parameters and free variables.
def f(a, b=2):
    c = 3

    def g():
        return c

    if a > 5:
        d = 1
    print(dir())
    return g


f(1)
f(9)
print((lambda q: dir())(0))


def outer():
    v = 1

    def inner():
        return dir(), v

    return inner()


print(outer())


def gen():
    w = 1
    yield dir()
    del w
    yield dir()


print(list(gen()))


# Class body: the namespace so far.
class A:
    y = 2

    def m(self):
        pass

    print(public(dir()))


a = A()
a.z = 3
print(public(dir(a)), public(dir(A)))
print('__dict__' in dir(A), '__module__' in dir(a), '__class__' in dir(a), '__init__' in dir(a))
print(dir(a) == sorted(object.__dir__(a)), dir(A) == sorted(type.__dir__(A)))


class B(A, dict):
    q = 0


print(public(dir(B)), 'keys' in dir(B()), 'z' in dir(B()))


# __dir__ overrides, also through super().
class C:
    def __dir__(self):
        return ('b', 'a', 'c', 'a')


print(dir(C()), type(dir(C())).__name__)


class D(A):
    def __dir__(self):
        return [n for n in super().__dir__() if not n.startswith('_')] + ['extra']


d = D()
d.k = 1
print(dir(d))


class Meta(type):
    def __dir__(cls):
        return ['meta'] + public(super().__dir__())


class E(A, metaclass=Meta):
    e = 1


print(dir(E), public(dir(E())))


class Bad:
    def __dir__(self):
        return 5


try:
    dir(Bad())
except TypeError as e:
    print('TypeError:', e)


class Mixed:
    def __dir__(self):
        return [1, 'a']


try:
    dir(Mixed())
except TypeError as e:
    print('TypeError:', e)

# Functions, bound methods (their function's attributes), generators.
def fn():
    yield 1


fn.tag = 1
A.m.mark = 2
print('tag' in dir(fn), '__code__' in dir(fn), 'mark' in dir(a.m), '__func__' in dir(a.m))
print([n for n in ('__next__', 'send', 'close', 'gi_running') if n not in dir(fn())])


async def co():
    pass


async def main():
    c = co()
    print('__await__' in dir(c), 'send' in dir(c))
    await c


asyncio.run(main())

# Modules.
print('sqrt' in dir(math), '__name__' in dir(math), dir(math) == sorted(dir(math)))

# Exceptions raised inside __dir__ propagate.
class Raises:
    def __dir__(self):
        raise KeyError('nope')


try:
    dir(Raises())
except KeyError as e:
    print('KeyError:', e)

try:
    dir(1, 2)
except TypeError as e:
    print('TypeError:', e)
try:
    dir(x=1)
except TypeError as e:
    print('TypeError:', e)
