# Natives that follow guest-built nesting must not overflow the Rust
# stack. `enumerate` and `zip` of each other read the chain with an
# explicit stack, as deep as CPython 3.14 goes (it recurses in C and
# crashes between 100,000 and 200,000 levels); a method of methods passes
# attribute lookups on in a loop; hashing a slice of slices or a method of
# methods is depth-counted like a tuple.
import types

N = 99_999


def leaf_digest(x):
    # Depth, leaf count and a checksum of the leaves, walked in a loop:
    # every level holds at most one tuple.
    depth = n = acc = 0
    while type(x) is tuple:
        depth += 1
        inner = None
        for y in x:
            if type(y) is tuple:
                inner = y
            else:
                n += 1
                acc = (acc * 31 + (y if type(y) is int else ord(y))) % 1000003
        x = inner
    return depth, n, acc


def chain(n, leaf, kind):
    e = leaf
    for i in range(n):
        k = kind[i % len(kind)]
        if k == 'e':
            e = enumerate(e, i)
        elif k == 'z':
            e = zip(e)
        elif k == 'Z':
            e = zip(range(i, i + 3), e, 'xyz'[: i % 4] or 'ab')
        else:
            e = zip(e, zip())
    return e


# Any nesting takes the walk; only the plain chains go as deep as N.
for kind in ('e', 'z', 'ez', 'Z', 'Ze', 'x', 'eeZ'):
    e = chain(N if kind in ('e', 'Z') else 1000, iter([10, 20, 30]), kind)
    print(kind, [leaf_digest(x) for x in e])

# Ways of reading one: next, for, list, str.join; and, on shallow ones
# (deep result tuples hash and compare only 200 levels deep), sum,
# unpacking, any, set, dict, in, sorted and max.
for n in (2, 3, N):
    def mk():
        return chain(n, iter('abc'), 'ez')
    e = mk()
    first = next(e)
    print(n, leaf_digest(first), [len(x) for x in e], next(e, 'end'))
    print(len(list(mk())), ''.join('x' for _ in chain(n, iter('ab'), 'z')))
    if n < 10:
        print(sum(1 for _ in mk()), len([*mk(), *mk()]), any(mk()))
        print(len(set(mk())), len(dict(zip(mk(), 'pqr'))), first in mk())
        print(leaf_digest(sorted(mk())[-1]), leaf_digest(max(mk())))

# zip stops at its first exhausted iterator; earlier ones keep their item.
a = iter([1, 2, 3])
b = enumerate(iter([7]))
print(list(zip(enumerate(zip(zip(a, b))), iter('pq'))), list(a), list(b))


def f():
    return 1


f.label = 'f'
m = f
for _ in range(N):
    m = types.MethodType(m, 1)
print(m.__name__, m.label, m.__func__.__self__, m.__self__)

# Hashing: equal shallow nests hash alike; deep ones never crash.
s = slice(slice(1, 2), (3, slice(4)), None)
print(hash(s) == hash(slice(slice(1, 2), (3, slice(4)), None)))
k = types.MethodType(types.MethodType(f, 1), 2)
print(hash(k) == hash(k), {k: 1}[k])
deep_s = 1
for _ in range(N):
    deep_s = slice(deep_s)
for x in (deep_s, m):
    try:
        hash(x)
    except RecursionError:
        pass  # micro-monty hashes 200 levels deep, as for tuples
    print('no crash')
