# An `__eq__` that inserts a key equal to the one being stored (the
# correctness review's dupins.py, AB1): the store finds it and does not add
# a second equal key.
class K:
    hook = None
    def __init__(self, v): self.v = v
    def __hash__(self): return 4242
    def __eq__(self, o):
        h, K.hook = K.hook, None
        if h: h()
        return isinstance(o, K) and self.v == o.v


for n in (4, 40, 63, 100, 200):
    d = {K(i): i for i in range(n)}
    K.hook = lambda: d.__setitem__(K(999), 'hook')
    d[K(999)] = 'main'
    print(n, 'store', len(d), d[K(999)])
    d = {K(i): i for i in range(n)}
    K.hook = lambda: d.__setitem__(K(999), 'hook')
    print(n, 'setdefault', d.setdefault(K(999), 'dflt'), len(d))
    d = {K(i): i for i in range(n)}
    K.hook = lambda: d.__setitem__(K(999), 'hook')
    print(n, 'get', d.get(K(999), 'dflt'), len(d))
    s = {K(i) for i in range(n)}
    K.hook = lambda: s.add(K(999))
    s.add(K(999))
    print(n, 'add', len(s))
