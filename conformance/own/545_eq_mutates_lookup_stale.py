# An `__eq__` that changes the container it is looked up in (the correctness
# review's stale_min.py, AB1): the lookup acts on the key it compared equal,
# never on an entry it handed out before the change.
class K:
    hook = None
    def __init__(self, v): self.v = v
    def __hash__(self): return 4242
    def __eq__(self, o):
        h, K.hook = K.hook, None
        if h: h()
        return isinstance(o, K) and self.v == o.v
    def __repr__(self): return 'K(%d)' % self.v


def build(n):
    d = {-i - 1: 'pad' for i in range(20)}
    for i in range(n): d[K(i)] = 'k%d' % i
    for i in range(20): del d[-i - 1]
    return d


def grow(c, m):
    return lambda: c.update(dict.fromkeys(range(m), 'int'))


def nks(c):
    return sum(1 for k in c if isinstance(k, K))


for n in (8, 40, 200):
    d = build(n); K.hook = grow(d, 20)
    print(n, 'get', d.get(K(n - 1)))
    d = build(n); K.hook = grow(d, 20)
    print(n, 'pop', d.pop(K(n - 1)), nks(d), len(d))
    d = build(n); K.hook = grow(d, 20)
    d[K(n - 1)] = 'NEW'
    print(n, 'set', d[K(n - 1)], d.get(19), nks(d), len(d))
    d = build(n); K.hook = grow(d, 20)
    del d[K(n - 1)]
    print(n, 'del', nks(d), len(d), K(n - 1) in d)
    d = build(n); K.hook = lambda: d.pop(K(n - 1))
    print(n, 'self-pop', d.get(K(n - 1), 'gone'), nks(d))
    s = {K(i) for i in range(n)}; K.hook = lambda: s.update(range(100))
    s.discard(K(n - 1))
    print(n, 'discard', len(s), nks(s), K(n - 1) in s)
    s = {K(i) for i in range(n)}; K.hook = lambda: s.update(range(100))
    print(n, 'in', K(n - 1) in s, K(n) in s)
