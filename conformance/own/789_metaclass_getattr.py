# A metaclass's `__getattr__` answers a class attribute that lookup does
# not find, as CPython's `slot_tp_getattr_hook`: on the class and its
# subclasses, through `getattr` with a default and `hasattr`, its own
# AttributeError passed on as raised. Attributes the class has, and the
# special methods an operation looks up on the type, never reach it.


class M(type):
    def __getattr__(cls, name):
        if name.startswith('x'):
            return (cls.__name__, name)
        if name == 'none':
            return None
        raise AttributeError(f'{cls.__name__} lacks {name}')


class K(metaclass=M):
    real = 1


class S(K):
    pass


def err(f):
    try:
        f()
    except AttributeError as e:
        return str(e)


assert K.xy == ('K', 'xy') and S.xz == ('S', 'xz') and K.none is None
assert getattr(K, 'none', 7) is None and hasattr(S, 'none') and getattr(K, 'q', 7) == 7
assert not hasattr(K, 'q') and err(lambda: S.q) == 'S lacks q'
assert K.real == 1 and K.__name__ == 'K' and S.__mro__ == (S, K, object)
for op in (lambda: len(K), lambda: K + 1, lambda: iter(K)):
    try:
        op()
    except TypeError:
        pass
    else:
        raise AssertionError('a special method reached __getattr__')
assert getattr(K(), 'xy', 0) == 0 and err(lambda: K().xq) == "'K' object has no attribute 'xq'"


class Plain(type):
    pass


class P(metaclass=Plain):
    pass


assert err(lambda: P.x) == "type object 'P' has no attribute 'x'" and getattr(P, 'x', 3) == 3
