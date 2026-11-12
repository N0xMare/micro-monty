# Star-unpacking of long lists and tuples ([*xs], (*xs,), f(*xs)) and
# math.hypot(*xs), which fold the argument tuple a slice per step.
import math
from math import hypot as h

N = 40_000
xs = [float(i % 997) / 7 for i in range(N)]
ints = [i % 1000 for i in range(N)]
t = tuple(xs)


def show(f):
    try:
        r = f()
    except Exception as e:
        r = (type(e).__name__, str(e))
    print(r)


def total(*a):
    return len(a), type(a).__name__, a[0], a[-1]


class L(list):
    pass


show(lambda: math.hypot(*xs))
show(lambda: math.hypot(*ints))
show(lambda: math.hypot(*t))
show(lambda: h(*xs))
show(lambda: math.hypot(*xs, *ints))
show(lambda: math.hypot(*xs, **{}))
show(lambda: math.hypot(*L(xs)))
show(lambda: math.hypot(*[1e300] * N))
show(lambda: math.hypot(*xs, float('inf'), float('nan')))
show(lambda: math.hypot(*xs, float('nan')))
show(lambda: math.hypot(*(['a'] * N)))
show(lambda: math.hypot(*xs, 'a'))
show(lambda: math.hypot(*xs, x=1))
show(lambda: math.hypot(*[3, 4]))
show(lambda: math.hypot(*range(N)))
show(lambda: total(*xs))
show(lambda: total(*t))
show(lambda: max(*ints))

a = [*xs]
b = [0, *xs, 1]
c = [*xs, *t]
d = (*xs,)
e = (*t, *range(3))
f = [*L(ints)]
print(len(a), a == xs, a is xs, type(a).__name__)
print(len(b), b[0], b[1], b[-2], b[-1])
print(len(c), c[N - 1], c[N], c == xs + xs)
print(len(d), type(d).__name__, d == t)
print(len(e), e[-4:])
print(len(f), f[:3], type(f).__name__)
ys = list(range(5))
ys.extend(t)
print(len(ys), ys[4], ys[5], ys[-1])
