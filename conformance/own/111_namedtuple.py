# collections.namedtuple, built without exec.
from collections import namedtuple

Point = namedtuple('Point', ['x', 'y'])
p = Point(1, y=2)
print(p, p.x, p.y, p[0], p._fields, tuple(p), p == (1, 2), isinstance(p, tuple))
print(p._asdict(), p._replace(y=5), Point._make([3, 4]), len(p))
x, y = p
print(x, y, Point.__name__, Point.x.__doc__, Point.__doc__)

P3 = namedtuple('P3', 'a b c', defaults=[10, 20])
print(P3(1), P3(1, 2), P3(1, 2, 3), P3._field_defaults)
R = namedtuple('R', ['ok', 'def', 'ok', '_x'], rename=True)
print(R._fields)

for args, kwargs in [((), {}), ((1, 2, 3), {}), ((1,), {'x': 2}), ((1, 2), {'z': 3})]:
    try:
        Point(*args, **kwargs)
    except TypeError as e:
        print('TypeError', e)

for bad in [['a', 'a'], ['a', 'def'], ['_a'], ['1a']]:
    try:
        namedtuple('B', bad)
    except ValueError as e:
        print('ValueError', e)

try:
    p._replace(z=1)
except TypeError as e:
    print('TypeError', e)
try:
    Point._make([1])
except TypeError as e:
    print('TypeError', e)


class Vec(namedtuple('Vec', 'x y')):
    def __add__(self, o):
        return Vec(self.x + o.x, self.y + o.y)

    @property
    def norm1(self):
        return abs(self.x) + abs(self.y)


v = Vec(1, 2) + Vec(3, -4)
print(v, v.norm1, sorted([Point(2, 1), Point(1, 5)]), max(Point(0, 1), Point(0, 2)))
print(hash(Point(1, 2)) == hash((1, 2)), {Point(1, 2): 'a'}[(1, 2)])
