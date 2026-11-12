# Comparisons too big for one native walk continue in the prelude, one
# level at a time; nesting must carry across those levels, so cyclic data
# raises RecursionError promptly and deep finite data still compares.


def chain(n, leaf='leaf'):
    x = leaf
    for _ in range(n):
        x = frozenset([x, 2])
    return x


def cyclic(item):
    c = [item]
    c.append(c)
    return c


def attempt(label, f):
    try:
        print(label, f())
    except RecursionError:
        print(label, 'RecursionError')


# Two self-containing lists, each item a 16-level frozenset chain
# (the libFuzzer nested_data finding).
a, b = cyclic(chain(16)), cyclic(chain(16))
attempt('lt', lambda: b < a)
attempt('eq', lambda: a == b)
attempt('ne', lambda: a != b)
attempt('max', lambda: max([b, a]) is not None)
attempt('sorted', lambda: len(sorted([b, a])))
attempt('in', lambda: a in [1, b])
attempt('tuple', lambda: (a, 1) == (b, 1))


# Deep but finite: 150 levels, each with enough equal items that one
# native walk cannot finish the comparison.
def deep(n, bottom):
    x = [bottom]
    for i in range(n):
        x = [str(j) for j in range(120)] + [x, chain(3)]
    return x


p, q = deep(150, 'end'), deep(150, 'end')
attempt('deep eq', lambda: p == q)
attempt('deep le', lambda: p <= q)
r = deep(150, 'other')
attempt('deep ne', lambda: p == r)
attempt('deep lt', lambda: p < r)
attempt('deep gt', lambda: r > p)
# Frozensets 150 deep whose levels each hold a 120-item tuple: too much
# for one walk, so the twin takes the set levels over.
def fchain(n, leaf):
    x = leaf
    for i in range(n):
        x = frozenset([x, tuple(str(j) for j in range(120))])
    return x


attempt('deep frozensets', lambda: fchain(150, 'a') == fchain(150, 'a'))
attempt('deep frozensets ne', lambda: fchain(150, 'a') == fchain(150, 'b'))
attempt('deep frozenset le', lambda: fchain(150, 'a') <= fchain(150, 'a'))
attempt('deep frozenset lt', lambda: fchain(150, 'a') < fchain(150, 'b'))


class E:
    def __init__(self, v):
        self.v = v

    def __eq__(self, other):
        return isinstance(other, E) and self.v == other.v

    __hash__ = None


attempt('deep user eq', lambda: deep(100, E(1)) == deep(100, E(1)))
attempt('deep user ne', lambda: deep(100, E(1)) == deep(100, E(2)))
try:
    deep(100, 1) < deep(100, 'x')
except TypeError as e:
    print('deep type', e)
