# Iteration protocol corners with user iterators.
class Seq:
    def __init__(self, n):
        self.n = n
        self.i = 0

    def __iter__(self):
        return self

    def __next__(self):
        self.i += 1
        if self.i > self.n:
            raise StopIteration(f'value-{self.i}')
        if self.i == 99:
            raise ValueError('never')
        return self.i


s = Seq(3)
print(iter(s) is s, list(s), list(s), next(s, 'default'))
a, b, c = Seq(3)
print(a, b, c)
try:
    x, y = Seq(3)
except ValueError as e:
    print(e)
try:
    x, y, z, w = Seq(3)
except ValueError as e:
    print(e)
first, *rest = Seq(4)
print(first, rest)
print(*Seq(2), sep='-')
print(dict(zip(Seq(3), 'abc')), set(Seq(2)), tuple(Seq(2)), sorted(Seq(3), reverse=True))
print(sum(Seq(4)), min(Seq(4)), max(Seq(4)), any(Seq(0)), all(Seq(2)), list(enumerate(Seq(2))))
print(list(map(str, Seq(3))), list(filter(lambda v: v > 1, Seq(3))), list(reversed(list(Seq(3)))))
print([v for v in Seq(3) if v != 2], {v: v for v in Seq(2)}, {v for v in Seq(2)})


class Broken:
    def __iter__(self):
        return self

    def __next__(self):
        raise KeyError('broken next')


try:
    for v in Broken():
        pass
except KeyError as e:
    print('KeyError', e)
try:
    list(Broken())
except KeyError as e:
    print('KeyError', e)
try:
    sum(Broken())
except KeyError as e:
    print('KeyError', e)


class IterReturnsList:
    def __iter__(self):
        return [1, 2]


try:
    for v in IterReturnsList():
        pass
except TypeError as e:
    print(e)


class Resettable:
    def __init__(self):
        self.data = [1, 2, 3]

    def __iter__(self):
        return iter(self.data)


r = Resettable()
print(list(r), list(r), 2 in r, [v * 10 for v in r])
it = iter(r)
print(next(it), list(it))


def gen_from_user():
    for v in Seq(3):
        yield v * 2


print(list(gen_from_user()), ''.join(str(v) for v in Seq(3)))
print(list(zip(Seq(5), Seq(2))))
loop = []
for i in Seq(3):
    for j in Seq(2):
        if j == 2:
            break
        loop.append((i, j))
    else:
        loop.append('no-break')
print(loop)
