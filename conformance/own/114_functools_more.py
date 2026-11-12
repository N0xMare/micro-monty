# functools beyond the corpus.
import functools


def deco(f):
    @functools.wraps(f)
    def inner(*a, **k):
        return f(*a, **k) + 1
    return inner


@deco
def answer(x):
    """Docs."""
    return x


print(answer(41), answer.__name__, answer.__doc__, answer.__wrapped__(1))


@functools.total_ordering
class Ver:
    def __init__(self, n):
        self.n = n

    def __eq__(self, o):
        return self.n == o.n

    def __lt__(self, o):
        return self.n < o.n


print(Ver(1) <= Ver(2), Ver(3) > Ver(2), Ver(2) >= Ver(2), Ver(1) > Ver(1))
print(sorted([3, 1, 2], key=functools.cmp_to_key(lambda a, b: b - a)))


class Circle:
    def __init__(self, r):
        self.r = r
        self.calls = 0

    @functools.cached_property
    def area(self):
        self.calls += 1
        return self.r * self.r * 3


c = Circle(2)
print(c.area, c.area, c.calls, 'area' in c.__dict__)


@functools.singledispatch
def describe(x):
    return 'thing'


@describe.register(int)
def _(x):
    return 'int'


@describe.register(list)
def _(x):
    return 'list'


print(describe(1), describe([1]), describe('s'), describe(True))


@functools.lru_cache(maxsize=2)
def sq(n):
    return n * n


print([sq(i) for i in (1, 2, 1, 3, 1)], sq.cache_info())
sq.cache_clear()
print(sq.cache_info())


class Cell:
    def set(self, key, value):
        return (key, value)

    set_x = functools.partialmethod(set, 'x')


print(Cell().set_x(5), functools.reduce(lambda a, b: a + b, ['a', 'b'], '>'))
p = functools.partial(functools.partial(max, 1), 5)
print(p(3), p.args, repr(functools.partial(divmod, 7)))
