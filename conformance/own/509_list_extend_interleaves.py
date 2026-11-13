# `list.extend`, list `+=` and `list.__init__` take an iterator's items as
# it yields them, as CPython's `list_extend_iter` does: code it runs sees
# the list grow, and an error keeps the items before it. `__init__`
# empties the list first. A list or tuple, or the list itself, is read as
# it was.
def g(a, n=3):
    for _ in range(n):
        yield len(a)


def boom(a):
    yield len(a)
    yield len(a)
    raise ValueError('boom')


a = [1]
a += g(a)
print(a)
a = [1]
a.extend(g(a))
print(a)
a = [1]
list.extend(a, g(a))
print(a)
a = [1]
a.extend(map(lambda x: len(a), range(3)))
print(a)
a = [1]
a.extend(zip(g(a), g(a)))
print(a)
a = [1]
a.extend(filter(None, g(a)))
print(a)
for f in (lambda a: a.extend(boom(a)), lambda a: a.__init__(boom(a)),
          lambda a: a.extend(map(int, ['2', 'x', '3'])), lambda a: a.extend(zip(boom(a), boom(a)))):
    a = [9]
    try:
        f(a)
    except ValueError as e:
        print('ValueError', a)
a = [1]
try:
    a += boom(a)
except ValueError:
    print('+=', a)
a = [1, 2]
a.__init__(a)
print(a)
a = [1, 2]
a.__init__(iter(a))
print(a)
a = [1, 2]
a.__init__(g(a))
print(a)
a = [1, 2]
a.__init__((3, 4))
print(a)
a = [1, 2]
a.extend(reversed(a))
a += reversed(a)
print(a)
a = [1, 2]
a.extend(a)
a += a
a.extend(enumerate(iter([5, 6])))
print(a)


class L(list):
    pass


l = L([1])
l += (len(l) for _ in range(3))
print(l, type(l).__name__)
l = L([1])
l.extend(len(l) for _ in range(3))
print(l)
# Long iterators: a chunk at a time, the rest in the prelude, parked between.
a = [0]
a.extend(iter(range(200_000)))
a += zip(range(150_000), range(150_000))
print(len(a), a[-1], sum(a[1:200_001]))


def count_up(a):
    for i in range(100_000):
        yield len(a) - i


a = []
a += count_up(a)
print(len(a), set(a))
