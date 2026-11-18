# held: allocation-heavy work while 20k generators are suspended (half at a
# plain yield, half inside a call window), so every collection meets them;
# then each is resumed to the end.
def total(*a):
    return sum(a)


def plain(i):
    row = [i, i + 1, i + 2]
    n = len(str(i) * 4)
    yield n
    yield sum(row) + n


def windowed(i):
    n = i % 7
    yield total(n, (yield n), 1, 2, 3)


gens = []
for i in range(10_000):
    p = plain(i)
    next(p)
    gens.append(p)
    w = windowed(i)
    next(w)
    gens.append(w)

kept = 0
for r in range(70):
    words = [str(i) * 6 for i in range(6_000)]
    index = {w: k for k, w in enumerate(words)}
    kept += len(index) + len(words[-1])

resumed = 0
for g in gens:
    resumed += g.send(5)
    for v in g:
        resumed += v
print(kept, resumed)
