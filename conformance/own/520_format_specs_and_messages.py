# CAP-2: Unicode decimal digits in a format spec's width and precision
# (not in `%`), `Py_ssize_t`'s limit on widths, precisions, field numbers
# and `[index]` keys, a bad code reported before a precision on an int
# and printed as CPython does; `str.replace`'s arity message (the clinic's,
# with `count=`; bytes and bytearray take no keywords); `__float__`'s
# message naming the class, its slot looked up as CPython's (a float
# subclass's result, which CPython takes with a warning, is not here); join's and
# `in`'s message when `iter()` refuses.
import math


def t(f):
    try:
        print(repr(f()))
    except Exception as e:
        print(type(e).__name__, e)


for spec in ('５', '٣', '0٣', '٠٣', '>٣.١f', '*^٥', '٣,', '𝟓', '²', '５' * 21, '9' * 19, '.' + '9' * 21,
             '9223372036854775807', '9223372036854775808', '.9223372036854775808', '.２f', '１０.３', '.٣ab'):
    for v in (7, 1.5, 'abcdef'):
        t(lambda: format(v, spec))
t(lambda: '%５d' % 1)
t(lambda: '%.２f' % 1.25)
t(lambda: f'{3:５}')
t(lambda: '{:.２f}'.format(1.25))
for fmt in ('{0:.2q}', '{999999999999999999999999999999999999}', '{0[999999999999999999999999999999999999]}',
            '{9999999999999999999}', '{0[9223372036854775807]}', '{0[٣]}', '{٣}', '{4294967296}',
            '{00000000000000000000001}', '{18446744073709551616}', '{0:.Q}', '{0:é}', '{0:\x1f}', '{0:.2é}'):
    t(lambda: fmt.format(1, 2, 3, 4))
    if '[' in fmt:
        t(lambda: fmt.format({'٣': 1}))
for code in 'é\x1fQ\x7f!~':
    t(lambda: format(1, code))
    t(lambda: format(1.0, code))
    t(lambda: format('s', code))
for f in (lambda: b'a'.replace(b'a'), lambda: bytearray(b'a').replace(b'a'), lambda: 'a'.replace(),
          lambda: 'a'.replace('a'), lambda: 'a'.replace('a', 'b', 1, 2), lambda: str.replace('a'),
          lambda: b'aa'.replace(b'a', b'b', count=1), lambda: bytearray(b'aa').replace(b'a', b'b', count=1),
          lambda: 'aa'.replace('a', 'b', count=1), lambda: b'aa'.replace(b'a', b'b', 1)):
    t(f)


class F(float):
    pass


class C:
    def __float__(self):
        return 1


class E(float):
    def __float__(self):
        return 3


class G:
    __float__ = float


class H:
    __float__ = None


class S:
    __float__ = staticmethod(lambda: 4.5)


class K:
    __float__ = classmethod(lambda cls: float(len(cls.__name__)))


class P:
    @property
    def __float__(self):
        return lambda: 6.5


for x in (C(), E(1.5), G(), H(), S(), K(), P()):
    t(lambda: float(x))
for x in (C(), G(), S(), K(), P()):
    t(lambda: math.sqrt(x))


class OptOut:
    __iter__ = None

    def __getitem__(self, i):
        return 1


class Bad:
    def __iter__(self):
        raise TypeError('mine')


for f in (lambda: ''.join(OptOut()), lambda: b''.join(OptOut()), lambda: 2 in OptOut(), lambda: ''.join(5),
          lambda: b''.join(5), lambda: 3 in 5, lambda: ''.join(Bad()), lambda: 1 in Bad(),
          lambda: ''.join(x for x in 'ab'), lambda: ''.join([1])):
    t(f)
try:
    ''.join(OptOut())
except TypeError as e:
    print(e.__context__, e.__cause__)
