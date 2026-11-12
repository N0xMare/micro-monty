# A bytearray's iterator is a builtin iterator like bytes': iterable,
# with `__next__`, searchable with `in`, accepted wherever an iterable is,
# and an `Iterator` to `collections.abc`.
import collections.abc
import itertools

ba = bytearray(b'abc')
print(list(map(abs, ba)))
x = []
x[:] = ba
print(x)
for v in iter(ba):
    print(v)
print(iter(ba).__next__())
print(98 in iter(ba), 120 in iter(ba))
it = iter(ba)
print(98 in it, list(it))
print(list(filter(None, bytearray(b'a\x00b'))))
print(list(itertools.chain(ba, ba)))
print(''.join(map(chr, ba)))
print(list(iter(ba)), tuple(iter(ba)), sum(iter(ba)))
print(list(zip(iter(ba), iter(ba))), list(enumerate(iter(ba))))
print(bytes(iter(ba)), bytearray(iter(ba)))
it = iter(ba)
print(iter(it) is it, next(it), next(it), next(it), next(it, 'end'))
print(type(iter(ba)).__name__, isinstance(iter(ba), collections.abc.Iterator))
print(isinstance(iter(ba), collections.abc.Iterable), issubclass(type(iter(ba)), collections.abc.Iterator))
print(sorted(iter(bytearray(b'cab'))), max(iter(ba)), min(iter(ba)), any(iter(ba)), all(iter(ba)))
print(set(iter(ba)) == {97, 98, 99}, dict.fromkeys(iter(ba)))
a, *rest = iter(ba)
print(a, rest)
print([*iter(ba)], {*iter(b'')})
try:
    next(iter(bytearray()))
except StopIteration:
    print('StopIteration')
# The iterator sees the bytearray as it is when each item is taken.
grow = bytearray(b'ab')
it = iter(grow)
print(next(it))
grow.extend(b'cd')
print(list(it))
