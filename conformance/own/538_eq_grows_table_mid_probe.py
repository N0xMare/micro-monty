# An `__eq__` that grows the set or dict while a lookup is past its first
# chunk of 64 colliding user-hashed keys: the lookup starts again in the
# new table, as CPython's does, so it finds a present key and an insert
# adds no second equal key (the correctness review's AA5).
class K:
    hook = None
    def __init__(self, v): self.v = v
    def __hash__(self): return 12345
    def __eq__(self, o):
        h, K.hook = K.hook, None
        if h: h()
        return isinstance(o, K) and self.v == o.v

for n in (60, 70, 200):
    ks = [K(i) for i in range(n)]
    s = set(ks)
    K.hook = lambda: s.update(range(100000, 105000))
    print(n, 'in', K(n - 1) in s)
    s = set(ks)
    K.hook = lambda: s.update(range(100000, 105000))
    s.add(K(n - 1))
    print(n, 'add', sum(1 for k in s if isinstance(k, K)), 'of', n)
    d = dict.fromkeys(ks, 0)
    K.hook = lambda: d.update(dict.fromkeys(range(100000, 105000)))
    d[K(n - 1)] = 5
    print(n, 'store', sum(1 for k in d if isinstance(k, K)), 'of', n, sorted(v for k, v in d.items() if isinstance(k, K))[-2:])
    d = dict.fromkeys(ks, 0)
    K.hook = lambda: d.update(dict.fromkeys(range(100000, 105000)))
    print(n, 'get', d.get(K(n - 1), 'missing'))
