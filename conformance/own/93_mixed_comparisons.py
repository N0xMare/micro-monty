# Comparisons across user classes and builtins; subclass priority; errors.
class Meters:
    def __init__(self, v):
        self.v = v

    def __eq__(self, o):
        print(f'Meters.__eq__({type(o).__name__})')
        if isinstance(o, Meters):
            return self.v == o.v
        if isinstance(o, (int, float)):
            return self.v == o
        return NotImplemented

    def __lt__(self, o):
        print(f'Meters.__lt__({type(o).__name__})')
        if isinstance(o, Meters):
            return self.v < o.v
        return NotImplemented

    __hash__ = None


class Km(Meters):
    def __eq__(self, o):
        print(f'Km.__eq__({type(o).__name__})')
        return NotImplemented

    def __gt__(self, o):
        print(f'Km.__gt__({type(o).__name__})')
        return True


m, k = Meters(5), Km(5000)
print(m == 5, 5 == m, m == 'x', 'x' == m)
print(m == k)
print(m < k)
print(k < m)
print(m != Meters(5), m != 6)
try:
    m < 3
except TypeError as e:
    print(e)
try:
    m >= m
except TypeError as e:
    print(e)


class Other:
    def __eq__(self, o):
        print('Other.__eq__')
        return 'truthy-string'


print(m == Other())
print(Other() == m)
if Other() == 1:
    print('non-bool eq used in if')
print([m] == [Meters(5)], m in [1, 2, Meters(5)], (m,) == (5,))
print(sorted([Meters(3), Meters(1)], key=lambda x: x.v)[0].v)


class Raises:
    def __eq__(self, o):
        raise ValueError('eq exploded')


try:
    Raises() in [1, 2]
except ValueError as e:
    print(e)
try:
    [1, 2].index(Raises())
except ValueError as e:
    print(e)
print(Raises() in [Raises.__name__] if False else 'skip', Raises() is not None)
x = Raises()
print(x in [x], [x] == [x], [x].count(x))
