# An object with `__index__` that is also iterable, passed where a builtin
# wants an int: `__index__` gives the int, the items are never read.
# `bytes()` and `bytearray()` ask for `__index__` before iterating, as
# CPython's `bytes_new`; `extend`, `int.from_bytes` (`PyObject_Bytes`) and
# the builtins that take an iterable read the items.
import math


class I:
    def __index__(self):
        return 2

    def __iter__(self):
        return iter([5, 6, 7])


o = I()
assert (hex(o), bin(o), oct(o)) == ('0x2', '0b10', '0o2')
assert range(o) == range(2) and list(range(1, 9, o)) == [1, 3, 5, 7]
assert chr(o) == '\x02' and [1, 2, 3].pop(o) == 3
assert 'abc'.zfill(o) == 'abc' and 'a'.ljust(o, '*') == 'a*'
assert 'a b c d'.split(' ', o) == ['a', 'b', 'c d']
assert round(1.555, o) == 1.55 and (5).to_bytes(o, 'big') == b'\x00\x05'
assert math.factorial(o) == 2 and math.comb(o, 1) == 2 and math.isqrt(o) == 1
xs = [1, 2, 3]
xs.insert(o, 9)
assert xs == [1, 2, 9, 3]
assert bytes(o) == b'\x00\x00' and bytearray(o) == bytearray(2)

assert list(o) == [5, 6, 7] and sum(o) == 18 and sorted(o, reverse=True) == [7, 6, 5]
ys = [0]
ys.extend(o)
assert ys == [0, 5, 6, 7]
ba = bytearray()
ba.extend(o)
assert ba == b'\x05\x06\x07'
assert int.from_bytes(o, 'big') == 0x050607


class Bad:
    def __iter__(self):
        raise TypeError('inner')


try:
    int.from_bytes(Bad(), 'big')
except TypeError as e:
    assert str(e) == "cannot convert 'Bad' object to bytes", e
else:
    raise AssertionError
