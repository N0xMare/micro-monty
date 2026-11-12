# Keyed sorted() and list.sort() run as native continuation steps
# (key calls through Cont::Native, a native merge of the keys).
xs = [(i * 7919) % 101 for i in range(30)]
print(sorted(xs, key=lambda x: -x))
print(sorted(xs, key=lambda x: x % 7))
print(sorted(xs, key=lambda x: x % 7, reverse=True))
print(sorted([], key=5))
print(sorted("hello", key=str.upper))
print(sorted({3: 1, 1: 2, 2: 3}, key=lambda k: -k))
l = ['bb', 'a', 'ccc', 'dd']
l.sort(key=len)
print(l)
l.sort(key=len, reverse=True)
print(l)
seen = []
l = [3, 1, 2]
l.sort(key=lambda x: (seen.append(list(l)), x)[1])
print(l, seen)
l = [3, 1, 2]
try:
    l.sort(key=lambda x: (l.append(9), x)[1])
except ValueError as e:
    print('ValueError', e, l)
def bad(x):
    if x == 2:
        raise KeyError(x)
    return x
l = [3, 1, 2, 0]
try:
    l.sort(key=bad)
except KeyError as e:
    print('KeyError', e, l)
try:
    sorted([1, 2, 3], key=bad)
except KeyError as e:
    print("KeyError2", e)
try:
    sorted([1, 'a', 2], key=lambda x: x)
except TypeError as e:
    print('TypeError', e)
l = [1, 'a', 2]
try:
    l.sort(key=lambda x: x)
except TypeError as e:
    print('TypeError', e, l)
class P:
    def __init__(self, v): self.v = v
    def __lt__(self, o): return self.v < o.v
    def __repr__(self): return f'P({self.v})'
print(sorted([3, 1, 2], key=P))
print(sorted([3, 1, 2], key=P, reverse=True))
l = [5, 4, 6]
l.sort(key=P)
print(l)
try:
    sorted([1, 2], key=5)
except TypeError as e:
    print('TypeError', e)
print(sorted(range(5), key=lambda x: (x % 2, -x)))
print(sorted(iter([3, 1, 2]), key=lambda x: x))
class It:
    def __init__(self): self.i = 0
    def __iter__(self): return self
    def __next__(self):
        self.i += 1
        if self.i > 3: raise StopIteration
        return 4 - self.i
print(sorted(It(), key=lambda x: x))
print(sorted([2, 1], key=None))
def gen_key(x):
    return x
print(min([3, 1, 2], key=gen_key), max([3, 1, 2], key=gen_key))
# A key that sorts many items (parks land inside key calls).
big = [(i * 7919) % 1009 for i in range(3000)]
r = sorted(big, key=lambda x: (x % 13, x))
print(r[:5], r[-5:], len(r))
big.sort(key=lambda x: -x)
print(big[:5])
# Keys that compare in Python fall back to the prelude merge.
print([p.v for p in sorted([P(i % 5) for i in range(20)], key=lambda p: p)][:8])
