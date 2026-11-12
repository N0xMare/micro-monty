# `for _ in range(...)` keeps its counters in registers; every other case
# takes the generic call and iterator.
def collect(*args):
    out = []
    for i in range(*args):
        out.append(i)
    return out


def direct():
    rows = []
    for i in range(4):
        rows.append(i)
    for i in range(2, 5):
        rows.append(i)
    for i in range(10, 0, -3):
        rows.append(i)
    for i in range(5, 5):
        rows.append('never')
    for i in range(3, 0):
        rows.append('never')
    for i in range(-2, 2, 1):
        rows.append(i)
    return rows


print(direct())
print(collect(3), collect(1, 7, 2))

# Near the small-int edge and past it.
edge = 2**47
print([i - edge for i in range(edge - 2, edge + 2)])
total = 0
for i in range(edge - 3, edge + 3, 2):
    total += i - edge
print(total)
for i in range(-edge - 1, -edge + 2):
    print(i + edge, end=' ')
print()

# Errors come from the call, with its position.
for bad in [(1.5,), ('3',), (1, 2, 0), (None,)]:
    try:
        for i in range(*bad):
            pass
    except (TypeError, ValueError) as e:
        print(type(e).__name__, e)
try:
    for i in range(0, 4, 0):
        pass
except ValueError as e:
    print('ValueError', e)
try:
    for i in range(2.0):
        pass
except TypeError as e:
    print('TypeError', e)

# bools and __index__ go through the generic path.
class Idx:
    def __index__(self):
        return 3


for i in range(True, Idx()):
    print('idx', i)

# A shadowed `range`, globally and locally.
def shadowed():
    def range(n):
        return ['a', 'b'][:n]

    return [x for x in ['x']] + [i for i in range(2)] + [i for i in collect(1)] + list_loop(range)


def list_loop(range):
    out = []
    for i in range(1):
        out.append(i)
    return out


print(shadowed())
real = range
range = lambda *a: iter(['shadow'])
for i in range(3):
    print(i)
range = real
del real

# The loop variable is a plain name: rebinding it does not steer the loop.
seen = []
for i in range(5):
    seen.append(i)
    i = 100
print(seen, i)

# break, continue, else, nesting.
for i in range(3):
    if i == 1:
        continue
    for j in range(i, 3):
        if j == 2:
            break
        print('pair', i, j)
else:
    print('done', i)
for i in range(3):
    if i == 1:
        break
else:
    print('never')
print('broke at', i)

# Inside generators, resumed between iterations.
def gen(n):
    for i in range(n):
        yield i * i


g = gen(4)
print(next(g), list(g))

# The range object itself is unchanged.
r = range(1, 10, 3)
for i in r:
    print('r', i)
print(r, list(r), len(r))
