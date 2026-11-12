# Bounded native steps for building big lists (docs/spec.md §5): a range
# gathered into a list or tuple goes a chunk per step past
# `COLLECT_CHUNK` (65,536) items, the rest resumed where it stopped; a
# full list of 1,048,576 items or more grows a slice per step on `append`
# and `insert`; a long sort's second merge array comes in a step of its
# own. Values must be CPython's.


def d(xs):
    return len(xs), xs[:3], xs[-3:], sum(xs[::4099])


N = 70_000
print(d(list(range(N))), d(tuple(range(-5, 3 * N, 3))))
print(d(list(range(N, -N, -7))), d(list(range(2**62, 2**62 + 3 * N, 3))))
# Built from the range in place while every item is a small int (48 bits).
print(d(tuple(range(2**47 - N, 2**47))), d(list(range(2**47 - N, 2**47 + 5))))
it = iter(range(2 * N))
next(it)
print(d(list(it)), list(it))
it = iter(range(N))
print(next(it), d(tuple(it)))
print(d(sorted(range(N, 0, -1))), d([*range(N)]), d(list(reversed(range(N)))))
a, *b, c = range(N)
print(a, len(b), c)


class L(list):
    pass


print(d(L(range(N))), type(L(range(N))).__name__)
xs = list(range(N))
xs.extend(range(N))
print(d(xs))

xs = list(range(1_048_576))
xs.append(-1)
xs.insert(len(xs), -2)
xs.insert(3, 's')
print(d(xs))
w = L(range(1_048_576))
w.append(0)
print(type(w).__name__, len(w), w[-2:])

ys = [(i * 7919) % 10_007 for i in range(20_000)]
ys.sort(reverse=True)
print(d(ys), d(sorted(ys, key=lambda v: -v)))
