# One call/attribute site seeing many classes and shapes (inline-cache misses).
class Circle:
    def __init__(self, r):
        self.r = r

    def area(self):
        return 3 * self.r * self.r

    name = 'circle'


class Square:
    def __init__(self, s):
        self.s = s

    def area(self):
        return self.s * self.s

    name = 'square'


class Rect(Square):
    def __init__(self, w, h):
        self.h = h
        super().__init__(w)

    def area(self):
        return self.s * self.h


class Weird:
    area = staticmethod(lambda: -1)
    name = property(lambda self: 'weird-prop')


class Dynamic:
    def __getattr__(self, n):
        if n == 'area':
            return lambda: 99
        return 'dyn-' + n


shapes = [Circle(1), Square(2), Rect(2, 3), Weird(), Dynamic(), Circle(2), Rect(1, 1)]
for _ in range(3):
    print([s.area() for s in shapes], [getattr(s, 'name', '?') for s in shapes])


def total(items):
    t = 0
    for it in items:
        t += it.area()
    return t


print(total(shapes * 5))


class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y


pts = []
for i in range(6):
    p = Point(i, -i)
    if i % 2:
        p.z = i * 100
    if i % 3 == 0:
        del p.x
        p.x = 'readded'
    pts.append(p)
print([(p.x, p.y, getattr(p, 'z', None)) for p in pts])
for p in pts:
    p.y = p.y * 2
print([p.y for p in pts], [list(vars(p)) for p in pts[:4]])


def read_x(o):
    return o.x


class HasX:
    x = 'class-x'


objs = [Point(1, 2), HasX(), Point(3, 4), HasX()]
objs[1].x = 'inst-x'
print([read_x(o) for o in objs])
HasX.x = 'changed'
print([read_x(o) for o in objs])
del objs[1].x
print([read_x(o) for o in objs])


class Meth:
    def m(self):
        return 'method'


ms = [Meth() for _ in range(3)]
ms[1].m = lambda: 'shadow'
print([o.m() for o in ms])
Meth.m = lambda self: 'replaced'
print([o.m() for o in ms])
del ms[1].m
print([o.m() for o in ms])
