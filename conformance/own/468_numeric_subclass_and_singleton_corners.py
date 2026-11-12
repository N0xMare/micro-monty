# Numeric subclass instances read their value's data attributes and abs();
# the singletons' types give their instance; math.pow converts with
# __float__; list and dict subclasses reverse with their base's __reversed__;
# the empty str and bytes hash to 0.
import math


def show(label, f):
    try:
        r = f()
    except BaseException as e:
        r = f'{type(e).__name__}: {e}'
    print(label, '->', repr(r))


class I(int):
    pass


class F(float):
    pass


class OwnAbs(int):
    def __abs__(self):
        return 'own'


class Real(int):
    @property
    def real(self):
        return 'property'


show('abs int sub', lambda: (abs(I(-5)), type(abs(I(-5))).__name__))
show('abs float sub', lambda: abs(F(-2.5)))
show('abs own', lambda: abs(OwnAbs(-1)))
show('int sub attrs', lambda: (I(5).real, type(I(5).real).__name__, I(5).imag, I(5).numerator, I(5).denominator))
show('float sub attrs', lambda: (F(2.5).real, type(F(2.5).real).__name__, F(2.5).imag))
show('conjugate', lambda: (I(5).conjugate(), F(1.5).conjugate()))
show('own property', lambda: Real(3).real)
show('missing still', lambda: I(1).nope)
show('bool', lambda: (True.real, True.imag, abs(True), abs(False)))
show('abs str sub', lambda: abs(type('S', (str,), {})('x')))

show('NoneType()', lambda: type(None)())
show('NoneType(1)', lambda: type(None)(1))
show('NoneType(x=1)', lambda: type(None)(x=1))
show('ellipsis()', lambda: type(...)())
show('ellipsis(1)', lambda: type(...)(1))
show('NotImplementedType()', lambda: type(NotImplemented)())
show('NotImplementedType(1)', lambda: type(NotImplemented)(1))
show('other type', lambda: type(len)())


class X:
    def __float__(self):
        return 3.0


class Ix:
    def __index__(self):
        return 2


show('math.pow __float__', lambda: math.pow(X(), 2))
show('math.pow both', lambda: math.pow(X(), X()))
show('math.pow __index__', lambda: math.pow(2, Ix()))
show('math.pow bad', lambda: math.pow('a', 2))
show('pow builtin', lambda: pow(X(), 2))


class L(list):
    def __getitem__(self, i):
        return 'over'

    def __len__(self):
        return 2


class D(dict):
    def __iter__(self):
        return iter(['x'])


class T(tuple):
    def __getitem__(self, i):
        return 'over'

    def __len__(self):
        return 2


class OwnRev(list):
    def __reversed__(self):
        return iter('own')


show('reversed list sub', lambda: list(reversed(L([1, 2, 3]))))
show('reversed dict sub', lambda: list(reversed(D(a=1, b=2))))
show('reversed tuple sub', lambda: list(reversed(T((1, 2, 3)))))
show('reversed own', lambda: list(reversed(OwnRev([1]))))
show('list.__reversed__', lambda: list(list.__reversed__([1, 2, 3])))
show('dict.__reversed__', lambda: list(dict.__reversed__({1: 2, 3: 4})))
show('bound', lambda: list([4, 5].__reversed__()))
show('has', lambda: ('__reversed__' in dir(list), hasattr(dict, '__reversed__'), hasattr(tuple, '__reversed__')))

# Functions of reals take what __index__ gives when there is no __float__.
import cmath
show('math of __index__', lambda: (math.sqrt(Ix()), math.floor(Ix()), math.isfinite(Ix()), math.atan2(Ix(), 1)))
show('cmath of __index__', lambda: (cmath.sqrt(Ix()), cmath.rect(Ix(), 0), cmath.phase(Ix())))
show('math of neither', lambda: math.sqrt(object()))

# The empty str and bytes hash to 0, so a key equal to '' that hashes to 0
# finds it.
class Empty:
    def __hash__(self):
        return 0

    def __eq__(self, other):
        return other == ''


show('hash empty', lambda: (hash(''), hash(b''), hash(''[:0]), hash(bytes(0)), hash('a'[1:])))
show('dict empty key', lambda: ({'': 'str'}[Empty()], {b'': 1}.get(b'')))
