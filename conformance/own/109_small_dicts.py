# Small dicts (no index) and their growth: semantics must not depend on layout.
eq_calls = []


class K:
    def __init__(self, h, name):
        self.h = h
        self.name = name

    def __hash__(self):
        return self.h

    def __eq__(self, other):
        eq_calls.append(self.name)
        return isinstance(other, K) and self.h == other.h

    def __repr__(self):
        return 'K(' + self.name + ')'


# Literals of every small size, then growth past the inline room.
for n in range(0, 11):
    d = {str(i): i for i in range(n)}
    lit = dict(zip([str(i) for i in range(n)], range(n)))
    assert d == lit
    for j in range(n, n + 12):
        d[str(j)] = j
    print(n, len(d), list(d)[:3], list(d)[-2:], sum(d.values()))

# Numeric keys that compare equal share an entry, in every layout.
d = {1: 'int'}
d[1.0] = 'float'
d[True] = 'bool'
print(d, d[1], d.get(1.0), True in d, 2 in d)
d = {0: 'a', 1.5: 'b', False: 'c', 'x': 'd'}
print(d, d[0.0], len(d))

# A key with a Python __hash__/__eq__ joins a small dict; __eq__ runs only
# on equal hashes, as in an indexed table.
d = {'a': 1, 'b': 2}
d[K(7, 'k1')] = 3
d[K(7, 'k2')] = 4
d[K(8, 'k3')] = 5
print(len(d), d[K(8, 'probe')], eq_calls)
eq_calls.clear()
small = {1: 'x', 'y': 2}
print(K(1, 'p') in small, eq_calls)
eq_calls.clear()
small = {1: 'x', 'y': 2}
print(small.get(K(hash('y'), 'q')), eq_calls)

# Deletions, reinsertions and order.
d = {'a': 1, 'b': 2, 'c': 3}
del d['b']
d['b'] = 4
d['d'] = 5
d['e'] = 6
print(d, list(d.items()))
d.pop('a')
d.popitem()
print(d, d.setdefault('z', 0), d)
d.clear()
d['q'] = 1
print(d, len(d))
for size in (1, 4, 8, 9):
    d = {i: i for i in range(size)}
    for i in range(size):
        del d[i]
        d[i + 100] = i
    print(size, d)

# Copies, equality, update, comprehension and unpacking.
a = {'x': 1, 'y': [1, 2]}
b = a.copy()
b['z'] = 3
print(a, b, a == {'y': [1, 2], 'x': 1}, a != b)
a.update(b, w=0)
print(a, {**a, 'x': 9}, dict(a, y=0))
print({k: v for k, v in zip('abcdefghij', range(10))})
print(sorted({(1, 2): 'tuple', 3.5: 'float', None: 'none'}.items(), key=repr))
print({'a': 1} | {'b': 2}, dict.fromkeys('abc', 0))

# Nested small dicts, as JSON rows build them.
rows = [{'id': i, 'name': 'n' + str(i), 'tags': {'t': i % 3}} for i in range(5)]
print(rows[3], [r['tags']['t'] for r in rows])
