# Private name mangling: inside a class body, `__x` (not ending in `__`) is
# `_Class__x` for names, attributes, parameters, def and class names, global
# declarations and imports, with the class name's leading underscores
# stripped; call keywords and class-pattern keywords are kept.


def show(label, f):
    try:
        r = f()
    except BaseException as e:
        r = f'{type(e).__name__}: {e}'
    print(label, '->', r)


# A subclass's `__x` does not overwrite its base's.
class A:
    def __init__(self):
        self.__x = 1

    def get(self):
        return self.__x


class B(A):
    def __init__(self):
        super().__init__()
        self.__x = 2

    def mine(self):
        return self.__x


b = B()
print(b.get(), b.mine(), sorted(vars(b)))
print(b._A__x, b._B__x)
show('outside', lambda: b.__x)
show('getattr string', lambda: getattr(b, '__x'))
print(getattr(b, '_A__x'), hasattr(b, '_B__x'))

# Private methods, class attributes and nested classes.
class _Base:
    __count = 0
    __x__ = 'dunder'
    ___y = 3

    def __helper(self):
        return 'base helper'

    def run(self):
        type(self).__count += 1
        return self.__helper(), self.__count

    class __Inner:
        __z = 'inner'

        def z(self):
            return self.__z


class Derived(_Base):
    def __helper(self):
        return 'derived helper'


print(sorted(k for k in vars(_Base) if not k.startswith('__')))
print(Derived().run(), _Base().run(), Derived._Base__count, Derived().__x__)
print(_Base._Base__helper.__name__, _Base._Base__helper.__qualname__)
inner = _Base._Base__Inner
print(inner.__name__, inner.__qualname__, inner().z(), sorted(k for k in vars(inner) if 'z' in k))

# Parameters, defaults, closures, lambdas, comprehensions and the walrus.
class P:
    def f(self, __p=3, *, __k=4):
        def __h():
            return __p
        return __h(), __k

    def scopes(self):
        return ((lambda __q: __q * 2)(5), [__i for __i in range(3)],
                {__k: __v for __k, __v in [('a', 1)]}, (__w := 7), __w)

    def keyword(self):
        return dict(__z=1)


print(P().f(), P().f(10), P().f(__p=1, __k=2) if False else 'skip')
print(P.f.__defaults__, P.f.__kwdefaults__)
show('keyword by name', lambda: P().f(__k=5))
show('keyword mangled', lambda: P().f(_P__k=5))
print(P().scopes(), P().keyword())

# global, del, augmented assignment and imports.
class G:
    def set(self):
        global __flag
        __flag = 'set'
        self.__n = 1
        self.__n += 41
        return self.__n

    def remove(self):
        del self.__n
        return hasattr(self, '_G__n')

    def imports(self):
        import json as __json
        from math import pi as __pi
        return __json.__name__, round(__pi, 2)

    def bad_import(self):
        import __nothing

    def bad_from(self):
        from math import __tau


g = G()
print(g.set(), g.remove(), _G__flag, g.imports())
show('import private', g.bad_import)
show('from import private', g.bad_from)

# Not mangled: all-underscore class names, names outside any class, and
# class-pattern keywords.
class ___:
    __z = 1


print([k for k in vars(___) if 'z' in k])
__top = 'module'


def outer():
    class C:
        def m(self):
            return __top
    return C


show('module private from a class', lambda: outer()().m())


class M:
    __v = 1

    def match(self, o):
        match o:
            case M(__v=1):
                return 'keyword pattern'
            case {'a': __a, **__rest}:
                return __a, __rest
            case [__s, *__t]:
                return __s, __t
        return 'none'


print(M().match(M()), M().match({'a': 1, 'b': 2}), M().match([1, 2, 3]))
print(M._M__v, '_M__v' in dir(M))
