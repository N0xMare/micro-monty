# A subscript that is a tuple holding slices (`a[1:2, ::3]`), as
# multi-dimensional containers take it: load, store, augmented store and
# del, with Ellipsis, None and a starred-free mix of slices and values.
class A:
    def __getitem__(self, k):
        return k
    def __setitem__(self, k, v):
        print('set', k, v)
    def __delitem__(self, k):
        print('del', k)


a = A()
print(a[1:2, ::3])
print(a[1:2, 5], a[..., 1:], a[:, None], a[::-1, 0:0, 'x'])
i, j = 1, 4
print(a[i:j, j:i:-1], a[i + 1:, :j * 2])
a[1:2, ::3] = 1
del a[::2, 1]
a[1:2] = 3


class B:
    def __init__(self):
        self.d = {}
    def __getitem__(self, k):
        return self.d.get(k, 0)
    def __setitem__(self, k, v):
        self.d[k] = v


b = B()
b[1:2, 3] += 5
b[1:2, 3] += 5
print(b.d)
print({(slice(1, 2, None), 1): 'x'}[1:2, 1])
try:
    {}[1:2, 3]
except KeyError as e:
    print('KeyError', e)
t = (1, 2)
try:
    t[0:1, 2]
except TypeError as e:
    print('TypeError', e)
