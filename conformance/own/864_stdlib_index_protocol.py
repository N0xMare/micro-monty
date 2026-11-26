import binascii
import bisect
import collections
import io
import itertools
import operator
import re


class Slot:
    def __get__(self, obj, owner):
        assert obj is not None and owner is Indexed
        calls.append(obj)
        return lambda: 2


class Indexed:
    __index__ = Slot()


x = Indexed()
x.__index__ = lambda: 99  # Special lookup ignores the instance attribute.
calls = []
assert operator.index(x) == 2
assert bisect.bisect_left([0, 1, 2, 3], 1, x) == 2
assert io.BytesIO(b'abc').seek(x) == 2
assert list(itertools.repeat('a', x)) == ['a', 'a']
assert collections.deque('abcd')[x] == 'c'
assert re.match('(a)(?P<b>b)', 'ab').group(x, 'b') == ('b', 'b')
assert binascii.hexlify(b'abc', b':', x) == b'61:6263'
assert calls == [x] * 7


class Integer(int):
    def __int__(self):
        raise AssertionError('called int override')

    def __index__(self):
        raise AssertionError('called index override')


y = operator.index(Integer(1 << 90))
assert type(y) is int and y == 1 << 90


class Invalid:
    def __index__(self):
        return 2.0


for use in (operator.index, lambda n: io.BytesIO().seek(n),
            lambda n: binascii.crc32(b'x', n)):
    try:
        use(Invalid())
    except TypeError as err:
        assert str(err) == '__index__ returned non-int (type float)'
    else:
        raise AssertionError('accepted a float from __index__')


class Iterable:
    def __iter__(self):
        raise AssertionError('iterated before converting bounds')


try:
    itertools.islice(Iterable(), Invalid())
except ValueError as err:
    assert str(err) == 'Stop argument for islice() must be None or an integer: 0 <= x <= sys.maxsize.'
else:
    raise AssertionError('accepted an invalid stop value')
