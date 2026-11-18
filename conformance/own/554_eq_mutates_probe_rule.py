# Lookups whose `__eq__` changes the container go on as CPython's do: from
# where they are, or again from the start when the table or the compared
# key was replaced (the lane K review's F1, F2, F3, F4 and F8). Every
# `__hash__`, `__eq__` and `__bool__` call is logged.
log = []


class B:
    def __init__(self, v): self.v = v

    def __bool__(self):
        log.append('b')
        return self.v


class K:
    act = None
    fire = 0
    n = 0
    h = 3
    def __init__(self, v, eqv=None): self.v = v; self.eqv = eqv

    def __hash__(self):
        log.append('h%d' % self.v)
        return K.h

    def __eq__(self, o):
        K.n += 1
        log.append('e%d' % self.v)
        if K.act and K.n >= K.fire:
            a, K.act = K.act, None
            a()
        if self.eqv is not None:
            return self.eqv
        return isinstance(o, K) and self.v == o.v

    def __repr__(self): return 'K%d' % self.v


def run(tag, f):
    del log[:]
    K.n = 0
    try:
        r = f()
    except Exception as e:
        r = type(e).__name__
    K.act = None
    print(tag, r, len(log), ' '.join(log[-12:]))


# F4: an `__eq__` stores a key equal to the one being stored while the keys
# Python compares fill whole chunks of 64, past deleted slots.
for n, ndel in ((64, 32), (63, 32), (64, 8), (128, 1), (192, 40)):
    for op in ('store', 'setdefault', 'get', 'pop', 'update', 'ior', 'add', 'in', 'discard'):
        ks = [K(i) for i in range(n + ndel)]
        dead = [ks[(j * 3 + 1) % len(ks)] for j in range(ndel)]
        isset = op in ('add', 'in', 'discard')
        if isset:
            c = set(ks)
            for k in dead: c.discard(k)
            K.act = lambda: c.add(K(1000))
        else:
            c = {k: 1 for k in ks}
            for k in dead: c.pop(k, None)
            K.act = lambda: c.__setitem__(K(1000), 'inner')
        x = K(1000)
        del log[:]
        if op == 'store': c[x] = 'outer'; r = None
        elif op == 'setdefault': r = c.setdefault(x, 'outer')
        elif op == 'get': r = c.get(x, 'miss')
        elif op == 'pop': r = c.pop(x, 'miss')
        elif op == 'update': c.update({x: 'outer'}); r = None
        elif op == 'ior': c |= dict.fromkeys([x], 'outer'); r = None
        elif op == 'add': c.add(x); r = None
        elif op == 'in': r = x in c
        else: c.discard(x); r = None
        K.act = None
        vals = [] if isset else [v for k, v in c.items() if k.v == 1000]
        print('F4', n, ndel, op, r, len(c), sum(1 for k in c if k.v == 1000), vals, len(log))

# F1: a set probe that comes to a slot twice compares its key twice.
for h in (3, 1 << 40):
    K.h = h
    for n in (12, 30, 70):
        s = {K(i) for i in range(n)}
        run('F1 in %d %d' % (h, n), lambda: K(-1) in s)
        run('F1 add %d %d' % (h, n), lambda: s.add(K(-2)))
        run('F1 discard %d %d' % (h, n), lambda: s.discard(K(-3)))
K.h = 3

# F2: an add whose compare is true has found the key, whatever the compare
# did to the set; a non-bool result's `__bool__` runs at once.
for n in (1, 4, 40):
    for act in ('clear', 'grow', 'none'):
        s = {K(i) for i in range(n)}
        if act == 'clear': K.act = s.clear
        elif act == 'grow': K.act = lambda: s.update(range(1000))
        run('F2 add %d %s' % (n, act), lambda: (s.add(K(999, True)), len(s))[1])
        s = {K(i) for i in range(n)}
        if act == 'clear': K.act = s.clear
        run('F2 update %d %s' % (n, act), lambda: (s.update([K(999, B(True))]), len(s))[1])

# F8: an `__eq__` that discards a key the probe passed and adds one equal to
# the looked-up key leaves the set's counts as they were.
for n in (6, 12, 30):
    for fire in (2, 3):
        for op in ('in', 'add', 'discard'):
            ks = [K(i) for i in range(n)]
            s = set(ks)
            o = list(s)
            s.discard(o[n - 1])
            def hook(s=s, p=o[0]):
                s.discard(p)
                s.add(K(99))
            K.act = hook
            K.fire = fire
            x = K(99)
            if op == 'in': run('F8 %d %d in' % (n, fire), lambda: x in s)
            elif op == 'add': run('F8 %d %d add' % (n, fire), lambda: s.add(x))
            else: run('F8 %d %d discard' % (n, fire), lambda: s.discard(x))
            print('  ', len(s), sum(1 for k in s if k.v == 99))
K.fire = 0

# F3: a keys view's `isdisjoint` hashes each item it tests.
K.h = 3
for nd, no in ((3, 2), (2, 5), (20, 3)):
    d = {K(i): i for i in range(nd)}
    ks = [K(100 + i) for i in range(no)]
    for tag, X in (('set', set(ks)), ('frozenset', frozenset(ks)), ('dict', dict.fromkeys(ks)),
                   ('keys', dict.fromkeys(ks).keys()), ('list', ks)):
        run('F3 %d %d %s' % (nd, no, tag), lambda: d.keys().isdisjoint(X))

# G1: an `__eq__` that clears the container and refills it to its size,
# with a key equal to the looked-up one where the walk has been. A table
# that a compare replaced starts the lookup again. CPython goes on in a new
# table at the old one's address, so only rows where its new table is
# surely another block are here: a second container of the same size takes
# the freed block first, which CPython's small-object allocator (and
# glibc's per-size cache under PYTHONMALLOC=malloc) hands out last-in
# first-out. That holds for tables of 512 bytes or less, dicts of up to 10
# keys and sets of up to 18; a bigger freed table may merge into free
# memory, and whether the refill gets the old address then follows the
# heap's state, not the program, so those sizes are left out (a set of 4
# keys or fewer keeps its inline table in both).
keep = []


class G:
    hook = None
    n = 0
    def __init__(self, v, eq=None): self.v = v; self.eq = eq
    def __hash__(self): return 7

    def __eq__(self, o):
        G.n += 1
        h = G.hook
        if h and h[0] is self:
            G.hook = None
            h[1]()
        if self.eq is not None and o is self.eq: return True
        return self is o


def refill(n, kind, op):
    ks = [G(i) for i in range(n)]
    X = G('X'); E = G('E', X)
    new = [E] + ks[1:]
    if kind == 'dict':
        c = {k: k.v for k in ks}
        def hook():
            c.clear()
            keep.append({k: 0 for k in ks})
            for k in new: c[k] = 0
    else:
        c = set(ks)
        def hook():
            c.clear()
            keep.append(set(ks))
            for k in new: c.add(k)
    G.hook = (ks[n // 2], hook)
    G.n = 0
    r = None
    if op == 'store':
        if kind == 'dict': c[X] = 'X'
        else: c.add(X)
    elif op == 'in': r = X in c
    elif kind == 'dict': r = c.pop(X, None)
    else: c.discard(X)
    G.hook = None
    del keep[:]
    print('G1', kind, n, op, len(c), sum(1 for k in c if k is E), sum(1 for k in c if k is X), r, G.n)


for kind, sizes in (('dict', (2, 3, 5, 6, 8, 9, 10)), ('set', (3, 4, 5, 8, 12, 16, 18))):
    for n in sizes:
        for op in ('store', 'in', 'pop'):
            refill(n, kind, op)
