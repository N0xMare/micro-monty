# Collecting a native iterator that need not end (zip, enumerate,
# reversed, the native items before a generator's) goes past a chunk of
# 65,536 items in the prelude, which parks between chunks: every caller
# must see the complete items, in order, and an iterator left part way
# consumed where CPython leaves it. `map` (a generator) is a control.
import itertools

N = 70_000


def M(n=N):
    return reversed(list(range(n - 1, -1, -1)))


def G(n=N):
    return map(abs, range(n))


def E(n=N):
    return enumerate(range(n))


print(len(list(E())), list(E())[-1], len(tuple(M())), sum(M()), max(M()), min(M(), key=lambda x: -x))
print(len(set(M())), len(frozenset(M())), len(dict(E())), len(dict.fromkeys(M())))
print(len(set(G())), sorted(G())[-1], len(dict(zip(G(), M()))), sum(G()), [*G(3)])
it = M()
d = dict(zip(it, it))
print(len(d), d[N - 2], next(it, 'end'))
print(sorted(map(lambda x: -x, range(N)))[:2], sorted(M(), reverse=True)[:2], sorted(M(), key=lambda x: -x)[-1])
print(len(','.join(map(str, M()))), len(bytes(map(lambda x: x % 256, range(N)))), len(bytearray(x % 256 for x in M())))
a, *b = M()
*c, e = E()
print(a, len(b), len(c), e, len([*M()]), len({*M()}), len((*M(), 1)))


def count(*args, **kw):
    return len(args), kw


print(count(*M(), x=1), count(*E(), 0, *M()))
l = [0]
l.extend(M())
l += E()
l[1:3] = M()
print(len(l), l[-1])
s = set()
s.update(M(), E())
print(len(s), len(set().union(M(), M(5))))
e = ValueError()
e.args = M()
print(len(e.args))
print(len(list(itertools.chain(range(N), (x for x in range(3))))), len(list(itertools.islice(itertools.count(), N))))
it = M(100_000)
print(len(list(itertools.islice(it, N))), len(list(it)), list(it))


def boom():
    yield from range(N)
    raise KeyError('mid')


for f in (list, tuple, set, sorted, sum, ''.join):
    try:
        f(map(str, boom()) if f == ''.join else boom())
    except KeyError as k:
        print('KeyError', k)
print(dict([(1, 2), iter([3, 4]), 'ab']))
try:
    dict([(1, 2), iter([3, 4]), M(N)])
except ValueError as v:
    print(v)
