# object.__init_subclass__ refusing keyword arguments names the class by
# its `__qualname__`, as CPython does: a class made in a function was named
# `K`, not `f.<locals>.K` (difffuzz). Nested in a class, renamed in its
# body, made by type(), with a metaclass, a base or a builtin base.
def f():
    class K(a=1):
        pass
def g():
    class Outer:
        class K(a=1):
            pass
def q():
    class Q(b=2):
        __qualname__ = 'Renamed.Q'
def h():
    return type('Dyn', (), {}, d=4)
def m():
    class Meta(type):
        pass
    class K(metaclass=Meta, e=5):
        pass
def n():
    class Base:
        pass
    class K(Base, f=6):
        pass
def s():
    class K(int, g=7):
        pass
def ok():
    class Base:
        def __init_subclass__(cls, **kw):
            print('init_subclass', cls.__qualname__, sorted(kw.items()))
    class K(Base, h=8):
        pass
    class L(K):
        pass
for fn in (f, g, q, h, m, n, s, ok):
    try:
        fn()
    except TypeError as e:
        print(fn.__name__, e.args)
class Top(k=9):
    pass
