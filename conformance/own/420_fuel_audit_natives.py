# Natives the fuel audit changed: `any`/`all` hand off to the prelude (past
# a fixed budget, or at an item with a user `__bool__`), `hash` of shared
# tuple nesting, popitem/pop order, slice deletion, byte search, unpacking,
# translate, padding limits and json over deleted entries.
import json


class B:
    def __init__(self, v):
        self.v = v

    def __bool__(self):
        print('bool', self.v)
        return self.v


class L:
    def __len__(self):
        return 0


print(any([0, B(False), 0, B(True), 1]), all([1, B(True), B(False), 0]))
print(any([0, L(), 0]), all([1, L()]))
print(any(iter(range(0, 1))), all(iter(range(1, 40_000))), all(iter(range(-20_000, 5))))
print(any(x > 30_000 for x in range(40_000)), any(map(int, '0' * 20_000 + '1')))
print(all(zip(range(20_000))), any(zip()), all(()), any(()))
it = iter(range(1, 40_000))
print(all(it), next(it, 'done'))
it = iter([0] * 20_000 + [7, 8])
print(any(it), next(it))


class Boom:
    def __bool__(self):
        raise ValueError('boom')


try:
    any([0] * 20_000 + [Boom()])
except ValueError as e:
    print('ValueError', e)
try:
    all(1 for _ in range(3)) and all([Boom()])
except ValueError as e:
    print('ValueError', e)

t = u = ()
for _ in range(60):
    t = (t, t)
for _ in range(60):
    u = (u, u)
print(hash(t) == hash(u), {t: 1}[t], t in {t}, hash((1, 2, (3, 4))) == hash((1, 2, (3, 4))))
deep = ()
for i in range(150):
    deep = (deep, i)
print(hash(deep) == hash(deep), hash(frozenset([t, 1])) == hash(frozenset([1, t])))
try:
    hash((t, [1]))
except TypeError as e:
    print('TypeError', e)

d = {i: i * i for i in range(10)}
del d[9], d[8], d[3]
d[20] = 1
print([d.popitem() for _ in range(4)], d)
for k in list(d)[3:]:
    del d[k]
print(d.popitem(), d.popitem(), d.popitem(), d)
try:
    d.popitem()
except KeyError as e:
    print('KeyError', e)
s = set(range(0, 200, 7))
print([s.pop() for _ in range(5)], len(s), sorted(s)[:3])

x = list(range(30))
del x[::3]
print(x)
del x[-1:2:-4]
print(x)
del x[100:]
del x[5:2]
print(x, len(x))
y = list(range(10))
del y[::-1]
print(y)

hay = b'ab' * 5000 + b'abc' + b'ab' * 5000
print(hay.find(b'ab' * 100 + b'c'), hay.rfind(b'b' + b'ab' * 99), b'abc' in hay, hay.count(b'aba'))
print(hay.find(b'ab' * 6000), (b'x' * 1000).find(b'x' * 999 + b'y'), b''.find(b''))
print(hay.index(b'cab'), hay.split(b'c')[1][:6], hay.replace(b'abc', b'!')[9998:10003])

try:
    a, b = list(range(10**6))
except ValueError as e:
    print('ValueError', e)
try:
    a, b = (1,)
except ValueError as e:
    print('ValueError', e)
a, (b, c), *r = [1, (2, 3), 4, 5]
print(a, b, c, r)

print('héllo wörld'.translate({ord('ö'): 'o', ord('é'): None, ord('l'): 'L'}))
print('abc'.translate(str.maketrans('ab', 'xy', 'c')))
for f in (str.center, str.ljust, str.rjust, str.zfill):
    try:
        f('x', 10**15)
    except MemoryError:
        print(f.__name__, 'MemoryError')
print('x'.center(7, '*'), '-5'.zfill(5), 'ab'.rjust(1))

e = {i: i for i in range(3000)}
for i in range(2990):
    del e[i]
print(json.dumps(e), json.dumps(list(range(5))), len(json.dumps(list(range(3000)))))
print(set(range(3000)) == set(range(3000)), e == dict(e), e.keys() == set(e), {1, 2} < {1, 2, 3})
print(sum([2] * 40_000), max([3] * 30_000 + [4]), min(range(5, 50_000)), 'a' * 200 < 'a' * 201)
big = 7 ** 3000
print((big & (big >> 3)) % 1000, (big | 1) % 1000, (big ^ big), big.bit_count() > 0, (-big).bit_length())
