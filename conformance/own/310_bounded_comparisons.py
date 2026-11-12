# Comparisons too large for one native walk: shared (DAG) and long data
# hand off to the prelude level by level and give the same results.


def dag(k, leaf):
    x = leaf
    for _ in range(k):
        x = [x, 1, x]
    return x


x, y, z = dag(15, 'leaf'), dag(15, 'leaf'), dag(15, 'leaf!')
print(x == y, x != y, x == z, x < z, z > x, x <= y, max([z, x]) is z)
print(sorted([z, x, y])[2] is z, min(z, x) is x, x in [z, y], [z, y].index(x))
print([z, x, y, x].count(y), (x, 1) == (y, 1), {'k': x} == {'k': y})

fs, gs = 'f', 'f'
for _ in range(13):
    fs = frozenset([fs, (fs, 1)])
    gs = frozenset([gs, (gs, 1)])
print(fs == gs, {fs: 1}[gs], gs in {fs}, {fs} <= {gs}, fs >= gs)

# Long flat data: native slices with a safepoint between them.
a = str(list(range(70000))).split(', ')
b = str(list(range(70000))).split(', ')
c = b[:-1] + ['x']
print(a == b, a == c, a < c, c > a, tuple(a) == tuple(b), tuple(a) < tuple(c))
da = dict.fromkeys(a, ['v'])
db = dict.fromkeys(b, ['v'])
print(da == db, da != db, {1: da} == {1: db}, [da, 1] < [db, 2])
db['5'] = ['6']
print(da == db, da != db, {1: da} == {1: db})

# Deep and large: levels whose subtree is too big compare in the prelude.
del a, b, c, da, db
p, q = 'end', 'end'
sp, sq = list(map(str, range(250))), list(map(str, range(250)))
for i in range(400):
    p = [p, sp]
    q = [q, sq]
print(p == q, p < q, [p] <= [q])

# Ordering a pair checks `==` first, as CPython does.
print([{1: 2}, 1] < [{1: 2}, 2], [{1}, 5] < [{2}, 0], [{1}] < [{1, 2}])
nan = float('nan')
print([float('nan')] < [float('nan')], [nan] <= [nan], [1, 2.0] == [1.0, 2])
try:
    [{1: 2}, 1] < [{1: 3}, 2]
except TypeError as e:
    print('TypeError', e)
try:
    [[1, 'a']] < [[1, 2]]
except TypeError as e:
    print('TypeError', e)


class Never(int):
    def __eq__(self, o):
        return False

    __hash__ = int.__hash__


print([Never(1)] < [Never(1)], [Never(1), 0] <= [Never(1), 1], [Never(1)] == [Never(1)])


class Same:
    def __eq__(self, o):
        return True

    __hash__ = None


# Items views compare like `all_contained_in`: values need not be hashable.
print({1: [Same()]}.items() == {1: [Same()]}.items(), {1: [Same()]}.items() == {2: [1]}.items())
