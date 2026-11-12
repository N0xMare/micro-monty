# The builtin iterators have `__next__` and `__iter__` as methods (they had
# only the protocol): `it.__next__()`, `next` through the bound method,
# `type(it).__next__(it)` and `__iter__` returning the iterator itself.
for x in [[1, 2], (1, 2), "ab", {1: 2, 3: 4}, {1, 2}, range(2), b"ab",
          {1: 2}.items(), {1: 2}.values(), enumerate([1, 2]), zip([1, 2], "ab"),
          reversed([1, 2])]:
    it = iter(x)
    print(type(it).__name__, hasattr(it, '__next__'), it.__iter__() is it)
    first = it.__next__()
    step = it.__next__
    rest = list(it)
    print(sorted([first] + rest, key=repr) if isinstance(x, set) else (first, rest))
    try:
        step()
    except StopIteration:
        print('exhausted')

it = iter([1, 2, 3])
print(type(it).__next__(it), next(it), list(it))
print([f() for f in [iter("xyz").__next__] * 1])

# a class built on a builtin iterator's `__next__`
class Wrap:
    def __init__(self, xs):
        self._next = iter(xs).__next__
    def __iter__(self):
        return self
    def __next__(self):
        return self._next() * 10
print(list(Wrap([1, 2, 3])))

g = (i for i in [1])
print(g.__next__())
