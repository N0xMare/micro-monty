# Arithmetic, reflected, in-place and unary dunders; NotImplemented fallbacks.
class V:
    def __init__(self, x, y):
        self.x, self.y = x, y

    def __repr__(self):
        return f'V({self.x}, {self.y})'

    def __add__(self, o):
        if isinstance(o, V):
            return V(self.x + o.x, self.y + o.y)
        if isinstance(o, (int, float)):
            return V(self.x + o, self.y + o)
        return NotImplemented

    __radd__ = __add__

    def __sub__(self, o):
        if not isinstance(o, V):
            return NotImplemented
        return V(self.x - o.x, self.y - o.y)

    def __mul__(self, k):
        return V(self.x * k, self.y * k)

    def __rmul__(self, k):
        return V(k * self.x, k * self.y)

    def __matmul__(self, o):
        return self.x * o.x + self.y * o.y

    def __neg__(self):
        return V(-self.x, -self.y)

    def __pos__(self):
        return self

    def __abs__(self):
        return (self.x ** 2 + self.y ** 2) ** 0.5

    def __invert__(self):
        return V(self.y, self.x)

    def __truediv__(self, k):
        return V(self.x / k, self.y / k)

    def __floordiv__(self, k):
        return V(self.x // k, self.y // k)

    def __mod__(self, k):
        return V(self.x % k, self.y % k)

    def __pow__(self, k):
        return V(self.x ** k, self.y ** k)


a, b = V(1, 2), V(3, 4)
print(a + b, a + 1, 1 + a, a - b, a * 3, 3 * a, a @ b)
print(-a, +a, abs(V(3, 4)), ~a, b / 2, b // 2, b % 2, a ** 2)
print(sum([a, b, V(1, 1)]), sum([a, b], V(0, 0)))
try:
    a - 1
except TypeError as e:
    print(e)
try:
    'x' + a
except TypeError as e:
    print(e)
try:
    a << 1
except TypeError as e:
    print(e)
try:
    -object()
except TypeError as e:
    print(e)


class Acc:
    def __init__(self):
        self.items = []

    def __iadd__(self, x):
        self.items.append(x)
        return self

    def __repr__(self):
        return f'Acc{self.items}'


acc = Acc()
keep = acc
acc += 1
acc += 2
print(acc, acc is keep)
v = V(1, 1)
w = v
v += V(1, 1)
print(v, w, v is w)
v *= 3
print(v)


class Left:
    def __add__(self, o):
        print('Left.__add__')
        return NotImplemented


class Right:
    def __radd__(self, o):
        print('Right.__radd__')
        return 'right wins'


print(Left() + Right())


class Base:
    def __mul__(self, o):
        return 'Base.__mul__'

    def __rmul__(self, o):
        return 'Base.__rmul__'


class Sub(Base):
    def __rmul__(self, o):
        return 'Sub.__rmul__'


print(Base() * Sub(), Sub() * Base(), 2 * Sub())


class Num:
    def __init__(self, n):
        self.n = n

    def __index__(self):
        return self.n

    def __int__(self):
        return self.n * 10

    def __float__(self):
        return self.n + 0.5


n = Num(2)
print([10, 20, 30][n], 'abc'[n:], int(n), float(n), bin(n), hex(n), range(n)[-1])
print(list(range(Num(3))), 'ab' * Num(2))
try:
    [1][Num]
except TypeError as e:
    print(e)
