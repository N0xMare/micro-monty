# An in-place `*=` or `+=` on a list, a list subclass's instance or a
# bytearray changes it and keeps it the same object, whatever the operand:
# an int, a bool, or an object with `__index__` as the count (which once
# rebound the name to a new object), and any iterable for a list's `+=`.
class N:
    def __index__(self):
        return 2


class R:
    def __rmul__(self, o):
        return NotImplemented

    def __index__(self):
        return 3


class S(list):
    def __iter__(self):
        return iter([9, 9])


class L(list):
    pass


def gen():
    yield 7


def check(name, x, op):
    y = x
    if op[0] == '*':
        x *= op[1]
    else:
        x += op[1]
    assert x is y, name
    print(name, x is y, type(x).__name__, x)


for make in (lambda: [1, 2], lambda: L([1, 2]), lambda: bytearray(b'ab')):
    for count in (2, True, 0, -1, N(), R()):
        check(f'{type(make()).__name__} *= {type(count).__name__}', make(), ('*', count))

for make in (lambda: [1, 2], lambda: L([1, 2])):
    for v in ([3], (3,), S([1]), gen(), L([3]), 'ab', {5: 1}, range(2), {4}, iter([6])):
        check(f'{type(make()).__name__} += {type(v).__name__}', make(), ('+', v))

for v in (b'c', bytearray(b'c')):
    check(f'bytearray += {type(v).__name__}', bytearray(b'ab'), ('+', v))

# A count on the left repeats the sequence, but not in place: only the left
# operand's repetition is in place, so `n *= seq` is unsupported.
for v in ([1], bytearray(b'a'), (1,), b'a', 'a'):
    for count in (N(), R()):
        w = v
        print(type(v).__name__, type(count).__name__, count * v, w)
        r = count
        try:
            r *= v
            print('no error', r)
        except TypeError as e:
            print(e)


# bytes and bytearray repeat by an object with arithmetic dunders as list does.
class F:
    def __rmul__(self, o):
        return NotImplemented


for v in (b'ab', bytearray(b'ab'), 'ab', [1]):
    print(v * R(), R() * v)
    try:
        v * F()
    except TypeError as e:
        print(e)


# `n *= seq` with an int on the left repeats `seq` into a new object and
# leaves it as it was (it once repeated `seq` in place).
row = [1, 2]
n = 2
n *= row
print(row, n, n is row)
ba = bytearray(b'ab')
m = 3
m *= ba
print(ba, m, m is ba)
x = L([1, 2])
k = 2
k *= x
k.append(99)
print(x, k, type(k).__name__)


# On the left of `*=`, an `__index__` object or an int subclass's instance
# repeats a sequence only through the right operand's own `__mul__` or
# `__rmul__`; an exact sequence or a subclass without them refuses.
class Sub(int):
    pass


class T(tuple):
    pass


class LM(list):
    def __mul__(self, o):
        return 'LM.mul'


class LIN(list):
    def __imul__(self, o):
        return NotImplemented


for left in (N(), Sub(3)):
    for v in ([1], T((1,)), 'a', LIN([1]), LM([1])):
        r = left
        try:
            r *= v
            print(type(left).__name__, type(v).__name__, type(r).__name__, r, v)
        except TypeError as e:
            print(e)

# An `__imul__` that declines falls back to `*`: a new plain list.
for count in (2, N()):
    x = LIN([1, 2])
    y = x
    x *= count
    print(type(x).__name__, x, x is y, y)


# A count past an index-sized integer is an OverflowError naming the
# count's type.
class Big:
    def __index__(self):
        return 1 << 70


for v in ([1], (1,), 'a'):
    for f in (lambda: v * Big(), lambda: Big() * v):
        try:
            f()
        except OverflowError as e:
            print(e)

# A count of one returns an exact tuple or bytes itself, and a subclass's
# instance a new value of the builtin type.
t = (1, 2)
b = b'ab'
s = T((1, 2))
print(t * True is t, b * 1 is b, type(s * 1).__name__)
