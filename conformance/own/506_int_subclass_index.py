# An int subclass's value is read without calling its `__index__`, as
# CPython's PyNumber_Index reads it; `int()` calls a user `__int__` and,
# only for a type without one, `__index__`, never an int or float
# subclass's own `__index__`.
import itertools
import math
import operator


class SI(int):
    def __index__(self):
        return 9


class SN(int):
    def __int__(self):
        return 8


class SB(int):
    def __index__(self):
        return 9

    def __int__(self):
        return 8


class X:
    def __index__(self):
        return 3


class Y:
    def __int__(self):
        return 4


class A(int):
    pass


class B(A):
    def __index__(self):
        return 9


class M:
    def __index__(self):
        return 9


class C(M, int):
    pass


class Late(int):
    pass


Late.__index__ = lambda self: 9


class F(float):
    def __index__(self):
        return 3


def show(f, v):
    try:
        r = f(v)
        return f'{type(r).__name__}:{r!r}'
    except Exception as e:
        return f'{type(e).__name__}:{e}'


vals = [SI(3), SN(3), SB(3), X(), Y(), B(3), C(3), Late(3), True, 3]
sites = [
    ('int', int), ('index', operator.index), ('float', float), ('complex', complex),
    ('trunc', lambda v: math.trunc(v) if isinstance(v, int) else 'skip'),
    ('range', lambda v: list(range(v))), ('range3', lambda v: list(range(0, 10, v))),
    ('getitem', lambda v: 'abcdefghij'[v]), ('slice', lambda v: 'abcdefghij'[v:]),
    ('lslice', lambda v: [0, 1, 2, 3, 4, 5][:v]), ('bslice', lambda v: b'abcdef'[v::2]),
    ('hex', hex), ('bin', bin), ('oct', oct),
    ('%d', lambda v: '%d' % v), ('%i', lambda v: '%i' % v), ('%x', lambda v: '%x' % v),
    ('fmt-x', lambda v: format(v, 'x')),
    ('fact', math.factorial), ('comb', lambda v: math.comb(v, 2)), ('perm', lambda v: math.perm(5, v)),
    ('isqrt', math.isqrt), ('gcd', lambda v: math.gcd(v, 6)),
    ('strmul', lambda v: 'ab' * v), ('lmul', lambda v: [0] * v), ('rmul', lambda v: v * 'ab'),
    ('bytes', lambda v: bytes(v) if not isinstance(v, Y) else 'skip'), ('chr', lambda v: chr(v + 62)),
    ('bytes-list', lambda v: (bytes([v, 65]), bytearray([v])) if isinstance(v, int) else 'skip'),
    ('round', lambda v: round(1.23456, v)), ('pop', lambda v: [0, 1, 2, 3, 4].pop(v)),
    ('lindex', lambda v: [1, 2, 3, 4, 5].index(4, v)), ('tindex', lambda v: (1, 2, 3, 4, 5).index(4, v)),
    ('islice', lambda v: list(itertools.islice('abcdef', v))), ('base', lambda v: int('11', v)),
    ('rjust', lambda v: 'a'.rjust(v)), ('split', lambda v: 'a b c d e f'.split(' ', v)),
    ('divmod', lambda v: divmod(10, v)), ('pow3', lambda v: pow(2, v, 100)),
    ('enum', lambda v: next(enumerate('a', v))[0]), ('add', lambda v: v + 1),
]
for name, f in sites:
    print(name, ' | '.join(show(f, v) for v in vals))

# An explicit call still reaches the method.
print(SI(3).__index__(), B(3).__index__(), Late(3).__index__(), SN(3).__int__())

# A float subclass converts as a float; `operator.index` calls its
# `__index__`.
print(show(int, F(2.5)), show(float, F(2.5)), show(math.trunc, F(2.5)), show(operator.index, F(2.5)))

# A class that becomes an int subclass's base, or gains `__index__` later.
class Grow(int):
    pass


g = Grow(2)
print('xyz'[g], hex(g))
Grow.__index__ = lambda self: 0
print('xyz'[g], hex(g), int(g), operator.index(g))
del Grow.__index__
print('xyz'[g], int(g))

# Big ints keep their value through the same paths.
big = [2**70, -2**70, SI(2**70), SN(-2**70), SB(2**64)]
print([operator.index(v) for v in big], [type(operator.index(v)).__name__ for v in big])
print([divmod(v, 7) for v in big], [pow(v, 3, 10**9 + 7) for v in big], [int(v) for v in big])
print(list(itertools.islice('ab', SI(2**62))), hex(SB(2**64)), [0, 1][SI(2**64) - 2**64])
